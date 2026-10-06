//! Content directory of a registry: the text that its constructor passes to the base constructor.
//!
//! Every template database constructor calls the shared base constructor with its content
//! directory as a `CString` argument. The method finds that call and establishes what the
//! argument register holds. Two compiled shapes exist:
//!
//! - **Temporary.** The constructor builds a `CString` on its stack from a text literal, then
//!   passes that stack object. Most registries have this shape.
//! - **Global.** The constructor passes a global `CString`. A static initializer elsewhere builds
//!   that global from a text literal. The source file then declares the path as a named constant
//!   (`common/ship_categories` on M45).
//!
//! The scan is linear and bounded. It tracks only constants, stack addresses and register copies.
//! A frame address, such as `x29 - 0x98` after the prologue sets `x29` from the stack pointer, is
//! a stack address.
//! A call clears the caller-saved registers, and any other instruction clears the register that
//! it may write. It does not follow branches, so a value that arrives on another path is unknown.
//! It stops at a return or an unconditional branch, since the next word is not reached from it.
//! When a base-constructor call follows such a jump, the arguments found before it are not a
//! proof: the call that the scan did not read may pass another directory.
//! Exactly one distinct directory over all constructor bodies names the registry. None or several
//! is a gap; the method never selects one of several.
//!
//! A database with a custom loader has no base-constructor directory. [`loader_directory`] takes
//! the text literal that the database's own functions pass to the file enumeration instead, under
//! the same rule: exactly one distinct literal, or a gap.
use std::collections::{BTreeMap, BTreeSet};

use super::decode::{add_immediate, adrp, sub_immediate};

/// Method revision recorded in each answer's source.
pub const METHOD: &str = "registry-directories/v3";

/// Base constructor whose `CString` argument is the content directory.
pub const BASE_CONSTRUCTOR: &str =
    "CSingleObjectGameDatabaseBase::CSingleObjectGameDatabaseBase(CString const&)";
/// Constructor that builds a `CString` from a text literal.
pub const STRING_CONSTRUCTOR: &str = "CString::CString(char const*)";
/// File enumeration whose first argument is a loader's content directory.
pub const FILE_ENUMERATION: &str =
    "VFSGetEnumeratedFiles(char const*, CPdxArray<CString, int>&, char const*, char const*, int)";
/// Name prefix of the static initializers that can build a global `CString`.
pub const INITIALIZER_PREFIX: &str = "__GLOBAL__sub_I_";

/// One function body.
#[derive(Debug, Clone)]
pub struct Constructor {
    /// File virtual address of the first instruction.
    pub address: u64,
    /// Complete function bytes.
    pub code: Vec<u8>,
}

/// Entry addresses of the two engine functions that give the scan its meaning.
#[derive(Debug, Clone, Default)]
pub struct Anchors {
    /// Every body of the base constructor.
    pub base_constructors: BTreeSet<u64>,
    /// Every body of the literal `CString` constructor.
    pub string_constructors: BTreeSet<u64>,
    /// Every body of the file enumeration.
    pub file_enumerations: BTreeSet<u64>,
}

/// What the constructors of one class establish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directory {
    /// Exactly one directory.
    Named(String),
    /// No base-constructor call, or an argument that the scan could not establish.
    Missing,
    /// Several distinct directories; none is selected.
    Ambiguous(Vec<String>),
}

/// What the argument register holds at one call to the base constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Argument {
    /// A `CString` that the same function built from this text literal.
    Literal(String),
    /// A global `CString` at this address. `globals` resolves it.
    Global(u64),
    /// Not established.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Value {
    Unknown,
    Constant(u64),
    Stack(u64),
}

/// A `CString` built from a literal: where it was built, and the literal's address.
struct Built {
    destination: Value,
    literal: u64,
}

/// The argument register at one base-constructor call.
enum Passed {
    /// A `CString` object that this function built from the literal at this address.
    Built(u64),
    /// Some other value: a global address, or unknown.
    Other(Value),
}

/// What one linear scan found.
#[derive(Default)]
struct Scan {
    /// Each literal `CString` construction.
    built: Vec<Built>,
    /// Each base-constructor call, with what its argument register holds.
    passed: Vec<Passed>,
    /// What the first argument holds at each file-enumeration call.
    enumerated: Vec<Value>,
    /// Whether a base-constructor call follows the jump that ended the scan, so the scan did not
    /// read its argument.
    unread_base_call: bool,
}

/// Linear scan of one function.
fn scan(function: &Constructor, anchors: &Anchors) -> Scan {
    let mut registers = [Value::Unknown; 32];
    // Objects built so far, by location. A later construction at one location replaces the
    // earlier one, and any other call that receives the object forgets it.
    let mut objects: Vec<(Value, u64)> = Vec::new();
    let mut found = Scan::default();
    let words = function.code.as_chunks::<4>().0;
    for (index, word) in words.iter().enumerate() {
        let word = u32::from_le_bytes(*word);
        let address = function.address + index as u64 * 4;
        if let Some((destination, page)) = adrp(word, address) {
            registers[destination] = Value::Constant(page);
        } else if let Some((destination, source, offset, sign)) = add_immediate(word)
            .map(|(destination, source, addend)| (destination, source, addend, 1))
            .or_else(|| {
                sub_immediate(word)
                    .map(|(destination, source, subtrahend)| (destination, source, subtrahend, -1))
            })
        {
            // Register 31 is the stack pointer: a source gives a stack address, and a write to it
            // is not followed.
            let offset = (offset as i64 * sign) as u64;
            let value = match (source, registers[source]) {
                (31, _) => Value::Stack(offset),
                (_, Value::Constant(base)) => Value::Constant(base.wrapping_add(offset)),
                (_, Value::Stack(base)) => Value::Stack(base.wrapping_add(offset)),
                _ => Value::Unknown,
            };
            if destination != 31 {
                registers[destination] = value;
            }
        } else if let Some((destination, source)) = move_register(word) {
            registers[destination] = registers[source];
        } else if let Some(target) = branch_with_link(word, address) {
            if anchors.file_enumerations.contains(&target) {
                found.enumerated.push(registers[0]);
            }
            if anchors.base_constructors.contains(&target) {
                found
                    .passed
                    .push(match objects.iter().find(|(at, _)| *at == registers[1]) {
                        Some(&(_, literal)) if registers[1] != Value::Unknown => {
                            Passed::Built(literal)
                        }
                        _ => Passed::Other(registers[1]),
                    });
            } else {
                objects.retain(|(at, _)| *at != registers[0]);
                if anchors.string_constructors.contains(&target)
                    && registers[0] != Value::Unknown
                    && let Value::Constant(literal) = registers[1]
                {
                    objects.push((registers[0], literal));
                    found.built.push(Built {
                        destination: registers[0],
                        literal,
                    });
                }
            }
            registers[..=18].fill(Value::Unknown);
        } else if let Some(transfer) = unconditional_branch(word) {
            match transfer {
                Transfer::Return => break,
                Transfer::Jump => {
                    let rest = &words[index + 1..];
                    found.unread_base_call =
                        calls_any(rest, address + 4, &anchors.base_constructors);
                    break;
                }
                // The callee is unknown, so it may receive or change any object.
                Transfer::IndirectCall => {
                    objects.clear();
                    registers[..=18].fill(Value::Unknown);
                }
            }
        } else {
            // Any other instruction may write its low register field, and a load or store with
            // writeback also writes its base register.
            if let Some(base) = writeback_base(word) {
                registers[base] = Value::Unknown;
            }
            registers[(word & 31) as usize] = Value::Unknown;
        }
    }
    found
}

/// The destination and source registers of a 64-bit register move, `mov xd, xm`, which is
/// `orr xd, xzr, xm`.
fn move_register(word: u32) -> Option<(usize, usize)> {
    (word & 0xffe0_ffe0 == 0xaa00_03e0)
        .then_some(((word & 31) as usize, (word >> 16 & 31) as usize))
}

/// The target of a `bl` word at `address`.
fn branch_with_link(word: u32, address: u64) -> Option<u64> {
    if word & 0xfc00_0000 != 0x9400_0000 {
        return None;
    }
    let offset = ((word & 0x03ff_ffff) as i64) << 38 >> 36;
    Some((address as i64).wrapping_add(offset) as u64)
}

/// Whether any `bl` in `words`, which start at `address`, calls one of `targets`.
fn calls_any(words: &[[u8; 4]], address: u64, targets: &BTreeSet<u64>) -> bool {
    words.iter().enumerate().any(|(index, word)| {
        let address = address + index as u64 * 4;
        branch_with_link(u32::from_le_bytes(*word), address)
            .is_some_and(|target| targets.contains(&target))
    })
}

/// What an unconditional branch does to the scan.
enum Transfer {
    /// `ret` or an authenticated return: the function ends.
    Return,
    /// `b`, `br` or another register branch without link: the scan ends, but code after it may
    /// still run.
    Jump,
    /// `blr` or an authenticated form: the scan continues after an unknown call.
    IndirectCall,
}

/// The transfer of an unconditional immediate or register branch word.
fn unconditional_branch(word: u32) -> Option<Transfer> {
    if word & 0xfc00_0000 == 0x1400_0000 {
        return Some(Transfer::Jump);
    }
    if word & 0xfe00_0000 != 0xd600_0000 {
        return None;
    }

    // Bits 21 to 23 of the register-branch opcode are 001 for `blr` and 010 for `ret`, with
    // their authenticated forms.
    Some(match word >> 21 & 0b111 {
        0b001 => Transfer::IndirectCall,
        0b010 => Transfer::Return,
        _ => Transfer::Jump,
    })
}

/// The base register of a pre- or post-indexed load or store of one register or a pair, which
/// writes the address back to its base.
fn writeback_base(word: u32) -> Option<usize> {
    let pair_writeback = word & 0x3a00_0000 == 0x2800_0000 && word & 0x0080_0000 != 0;
    let single_writeback = word & 0x3b20_0400 == 0x3800_0400;
    (pair_writeback || single_writeback).then_some((word >> 5 & 31) as usize)
}

/// The literal as a directory path without trailing `/`, when that path is nonempty.
fn directory_path(strings: &BTreeMap<u64, String>, literal: u64) -> Option<String> {
    let path = strings.get(&literal)?.trim_end_matches('/');
    (!path.is_empty()).then(|| path.to_owned())
}

/// The argument of every base-constructor call in one constructor body.
pub fn arguments(
    constructor: &Constructor,
    anchors: &Anchors,
    strings: &BTreeMap<u64, String>,
) -> Vec<Argument> {
    let scan = scan(constructor, anchors);
    // Arguments read before a jump do not prove the directory when the code after it, which the
    // scan did not follow, calls the base constructor again.
    let unread = (scan.unread_base_call && !scan.passed.is_empty()).then_some(Argument::Unknown);
    scan.passed
        .into_iter()
        .map(|passed| match passed {
            Passed::Built(literal) => {
                directory_path(strings, literal).map_or(Argument::Unknown, Argument::Literal)
            }
            Passed::Other(Value::Constant(global)) => Argument::Global(global),
            Passed::Other(_) => Argument::Unknown,
        })
        .chain(unread)
        .collect()
}

/// Global `CString` objects that the static initializers build from text literals. A global that
/// is built more than once with different text is left out.
pub fn globals(
    initializers: &[Constructor],
    anchors: &Anchors,
    strings: &BTreeMap<u64, String>,
) -> BTreeMap<u64, String> {
    let mut found: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
    for initializer in initializers {
        for built in scan(initializer, anchors).built {
            if let (Value::Constant(global), Some(path)) =
                (built.destination, directory_path(strings, built.literal))
            {
                found.entry(global).or_default().insert(path);
            }
        }
    }
    found
        .into_iter()
        .filter(|(_, texts)| texts.len() == 1)
        .map(|(global, mut texts)| (global, texts.pop_first().expect("one text")))
        .collect()
}

/// The directory that a custom loader enumerates: the literal first argument of every
/// file-enumeration call in `functions`, the database's own functions.
pub fn loader_directory(
    functions: &[Constructor],
    anchors: &Anchors,
    strings: &BTreeMap<u64, String>,
) -> Directory {
    let arguments: Vec<Argument> = functions
        .iter()
        .flat_map(|function| scan(function, anchors).enumerated)
        .map(|value| match value {
            Value::Constant(literal) => {
                directory_path(strings, literal).map_or(Argument::Unknown, Argument::Literal)
            }
            _ => Argument::Unknown,
        })
        .collect();

    directory(&arguments, &BTreeMap::new())
}

/// Resolve the directory from the arguments of every constructor body of one database class.
pub fn directory(arguments: &[Argument], globals: &BTreeMap<u64, String>) -> Directory {
    let mut found = BTreeSet::new();
    for argument in arguments {
        match argument {
            Argument::Literal(text) => found.insert(text.clone()),
            Argument::Global(global) => match globals.get(global) {
                Some(text) => found.insert(text.clone()),
                None => return Directory::Missing,
            },
            Argument::Unknown => return Directory::Missing,
        };
    }
    let mut found: Vec<_> = found.into_iter().collect();
    match found.len() {
        0 => Directory::Missing,
        1 => Directory::Named(found.remove(0)),
        _ => Directory::Ambiguous(found),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 0x1100;
    const STRING: u64 = 0x1200;
    const ENUMERATE: u64 = 0x1300;
    const NOP: u32 = 0xd503_201f;
    // adrp x1, one page after the function at 0x1000.
    const ADRP_X1: u32 = 0xb000_0001;
    const ADD_X0_SP_8: u32 = 0x9100_23e0;
    const ADD_X1_SP_8: u32 = 0x9100_23e1;
    const MOV_X0_X22: u32 = 0xaa16_03e0;
    const RET: u32 = 0xd65f_03c0;
    const BR_X8: u32 = 0xd61f_0100;
    const BLR_X8: u32 = 0xd63f_0100;
    // b to 0x100 bytes ahead.
    const B_AHEAD: u32 = 0x1400_0040;

    fn add_x1(immediate: u32) -> u32 {
        0x9100_0021 | immediate << 10
    }
    // adrp x0, one page after the function at 0x1000.
    const ADRP_X0: u32 = 0xb000_0000;
    fn add_x0(immediate: u32) -> u32 {
        0x9100_0000 | immediate << 10
    }
    /// A custom loader that enumerates the files under the literal at `literal`.
    fn loader(literal: u32) -> Constructor {
        function(&[ADRP_X0, add_x0(literal), bl(2, ENUMERATE)])
    }
    fn bl(from_index: u64, target: u64) -> u32 {
        let offset = (target as i64 - (0x1000 + from_index as i64 * 4)) / 4;
        0x9400_0000 | (offset as u32 & 0x03ff_ffff)
    }
    fn function(words: &[u32]) -> Constructor {
        Constructor {
            address: 0x1000,
            code: words.iter().flat_map(|word| word.to_le_bytes()).collect(),
        }
    }
    fn anchors() -> Anchors {
        Anchors {
            base_constructors: BTreeSet::from([BASE]),
            string_constructors: BTreeSet::from([STRING]),
            file_enumerations: BTreeSet::from([ENUMERATE]),
        }
    }
    fn strings() -> BTreeMap<u64, String> {
        BTreeMap::from([
            (0x2010, "common/examples".to_owned()),
            (0x2020, "map/other/".to_owned()),
        ])
    }
    /// The ordinary shape: literal, stack `CString`, base constructor.
    fn temporary(literal: u32) -> Constructor {
        function(&[
            ADRP_X1,
            add_x1(literal),
            ADD_X0_SP_8,
            bl(3, STRING),
            NOP,
            ADD_X1_SP_8,
            bl(6, BASE),
        ])
    }
    fn resolve(constructors: &[Constructor], globals: &BTreeMap<u64, String>) -> Directory {
        let arguments: Vec<_> = constructors
            .iter()
            .flat_map(|c| arguments(c, &anchors(), &strings()))
            .collect();
        directory(&arguments, globals)
    }

    #[test]
    fn a_custom_loader_is_named_by_the_one_literal_it_enumerates() {
        let named = loader_directory(&[loader(0x10), loader(0x10)], &anchors(), &strings());
        assert_eq!(named, Directory::Named("common/examples".into()));

        let several = loader_directory(&[loader(0x10), loader(0x20)], &anchors(), &strings());
        assert_eq!(
            several,
            Directory::Ambiguous(vec!["common/examples".into(), "map/other".into()])
        );

        let unknown = function(&[ADD_X0_SP_8, bl(1, ENUMERATE)]);
        assert_eq!(
            loader_directory(&[loader(0x10), unknown], &anchors(), &strings()),
            Directory::Missing
        );
        assert_eq!(
            loader_directory(&[temporary(0x10)], &anchors(), &strings()),
            Directory::Missing
        );
    }

    #[test]
    fn a_temporary_built_from_a_literal_names_the_registry() {
        assert_eq!(
            resolve(&[temporary(0x10)], &BTreeMap::new()),
            Directory::Named("common/examples".into())
        );
        assert_eq!(
            resolve(&[temporary(0x20)], &BTreeMap::new()),
            Directory::Named("map/other".into())
        );
    }

    #[test]
    fn a_temporary_at_a_frame_address_names_the_registry() {
        // add x29, sp, #0x40; sub x0, x29, #0x20; sub x1, x29, #0x20
        let add_x29_sp: u32 = 0x9101_03fd;
        let sub_x0_x29: u32 = 0xd100_83a0;
        let sub_x1_x29: u32 = 0xd100_83a1;
        let framed = function(&[
            add_x29_sp,
            ADRP_X1,
            add_x1(0x10),
            sub_x0_x29,
            bl(4, STRING),
            sub_x1_x29,
            bl(6, BASE),
        ]);
        assert_eq!(
            resolve(&[framed], &BTreeMap::new()),
            Directory::Named("common/examples".into())
        );

        let elsewhere = function(&[
            add_x29_sp,
            ADRP_X1,
            add_x1(0x10),
            sub_x0_x29,
            bl(4, STRING),
            ADD_X1_SP_8,
            bl(6, BASE),
        ]);
        assert_eq!(resolve(&[elsewhere], &BTreeMap::new()), Directory::Missing);
    }

    #[test]
    fn a_literal_that_is_not_passed_to_the_base_constructor_names_nothing() {
        let unrelated = function(&[ADRP_X1, add_x1(0x10), ADD_X0_SP_8, bl(3, STRING)]);
        assert_eq!(resolve(&[unrelated], &BTreeMap::new()), Directory::Missing);
        assert_eq!(resolve(&[], &BTreeMap::new()), Directory::Missing);
    }

    #[test]
    fn a_global_argument_is_resolved_through_its_static_initializer() {
        // adrp x19; add x19, x19, #0x800; add x22, x19, #0x28; literal in x1; mov x0, x22; bl.
        let initializer = function(&[
            0xb000_0013,
            0x9120_0273,
            0x9100_a276,
            ADRP_X1,
            add_x1(0x10),
            MOV_X0_X22,
            bl(6, STRING),
        ]);
        let globals = globals(&[initializer], &anchors(), &strings());
        assert_eq!(
            globals,
            BTreeMap::from([(0x2828, "common/examples".into())])
        );
        // adrp x1; add x1, x1, #0x828; bl base.
        let constructor = function(&[ADRP_X1, add_x1(0x828), bl(2, BASE)]);
        assert_eq!(
            resolve(std::slice::from_ref(&constructor), &globals),
            Directory::Named("common/examples".into())
        );
        assert_eq!(
            resolve(&[constructor], &BTreeMap::new()),
            Directory::Missing
        );
    }

    #[test]
    fn several_directories_are_never_resolved_to_one() {
        assert_eq!(
            resolve(&[temporary(0x10), temporary(0x20)], &BTreeMap::new()),
            Directory::Ambiguous(vec!["common/examples".into(), "map/other".into()])
        );
        assert_eq!(
            resolve(&[temporary(0x10), temporary(0x10)], &BTreeMap::new()),
            Directory::Named("common/examples".into())
        );
    }

    #[test]
    fn a_call_or_a_write_between_the_steps_clears_the_tracked_value() {
        // A call between the literal and the CString constructor clears x1.
        let call = function(&[
            ADRP_X1,
            add_x1(0x10),
            bl(2, 0x1300),
            ADD_X0_SP_8,
            bl(4, STRING),
        ]);
        assert!(scan(&call, &anchors()).built.is_empty());
        // mov x1, x2 replaces the stack argument before the base constructor.
        let mut words = vec![
            ADRP_X1,
            add_x1(0x10),
            ADD_X0_SP_8,
            bl(3, STRING),
            ADD_X1_SP_8,
        ];
        words.extend([0xaa02_03e1, bl(6, BASE)]);
        assert_eq!(
            resolve(&[function(&words)], &BTreeMap::new()),
            Directory::Missing
        );
    }

    #[test]
    fn an_indirect_call_forgets_every_built_object() {
        let words = [
            ADRP_X1,
            add_x1(0x10),
            ADD_X0_SP_8,
            bl(3, STRING),
            BLR_X8,
            ADD_X1_SP_8,
            bl(6, BASE),
        ];
        assert_eq!(
            resolve(&[function(&words)], &BTreeMap::new()),
            Directory::Missing
        );
    }

    #[test]
    fn a_base_constructor_call_after_a_jump_leaves_the_earlier_argument_unproven() {
        let complete = temporary(0x10).code;
        let skipped_call = [
            ADRP_X1,
            add_x1(0x20),
            ADD_X0_SP_8,
            bl(11, STRING),
            NOP,
            ADD_X1_SP_8,
            bl(14, BASE),
        ];
        let ends = |end: u32, rest: &[u32]| {
            let mut code = complete.clone();
            code.extend(
                std::iter::once(end)
                    .chain(rest.iter().copied())
                    .flat_map(u32::to_le_bytes),
            );
            Constructor {
                address: 0x1000,
                code,
            }
        };

        assert_eq!(
            resolve(&[ends(B_AHEAD, &skipped_call)], &BTreeMap::new()),
            Directory::Missing
        );
        assert_eq!(
            resolve(&[ends(RET, &skipped_call)], &BTreeMap::new()),
            Directory::Named("common/examples".into())
        );
        assert_eq!(
            resolve(&[ends(B_AHEAD, &[NOP])], &BTreeMap::new()),
            Directory::Named("common/examples".into())
        );
    }

    #[test]
    fn a_return_or_unconditional_branch_ends_the_scan() {
        for stop in [RET, BR_X8, B_AHEAD] {
            let words = [
                stop,
                ADRP_X1,
                add_x1(0x10),
                ADD_X0_SP_8,
                bl(4, STRING),
                ADD_X1_SP_8,
                bl(6, BASE),
            ];
            assert_eq!(
                resolve(&[function(&words)], &BTreeMap::new()),
                Directory::Missing
            );
        }
    }
}
