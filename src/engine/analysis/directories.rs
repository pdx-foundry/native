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
//! A call clears the caller-saved registers, and any other instruction clears the register that
//! it may write. It does not follow branches, so a value that arrives on another path is unknown.
//! Exactly one distinct directory over all constructor bodies names the registry. None or several
//! is a gap; the method never selects one of several.
use std::collections::{BTreeMap, BTreeSet};

/// Method revision recorded in each answer's source.
pub const METHOD: &str = "registry-directories/v2";

/// Base constructor whose `CString` argument is the content directory.
pub const BASE_CONSTRUCTOR: &str =
    "CSingleObjectGameDatabaseBase::CSingleObjectGameDatabaseBase(CString const&)";
/// Constructor that builds a `CString` from a text literal.
pub const STRING_CONSTRUCTOR: &str = "CString::CString(char const*)";
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

/// Linear scan of one function. Reports each literal `CString` construction, and each call to the
/// base constructor with what its argument register holds at that call.
fn scan(function: &Constructor, anchors: &Anchors) -> (Vec<Built>, Vec<Passed>) {
    let mut registers = [Value::Unknown; 32];
    // Objects built so far, by location. A later construction at one location replaces the
    // earlier one, and any other call that receives the object forgets it.
    let mut objects: Vec<(Value, u64)> = Vec::new();
    let mut built = Vec::new();
    let mut passed = Vec::new();
    for (index, word) in function.code.chunks_exact(4).enumerate() {
        let word = u32::from_le_bytes(word.try_into().expect("four bytes"));
        let address = function.address + index as u64 * 4;
        let (rd, rn) = ((word & 31) as usize, ((word >> 5) & 31) as usize);
        if word & 0x9f00_0000 == 0x9000_0000 {
            // adrp xd, page
            let immediate = (((word >> 5) & 0x7ffff) << 2 | ((word >> 29) & 3)) as i64;
            let immediate = if immediate >= 1 << 20 {
                immediate - (1 << 21)
            } else {
                immediate
            };
            let page = ((address & !0xfff) as i64).wrapping_add(immediate << 12);
            registers[rd] = Value::Constant(page as u64);
        } else if word & 0xff80_0000 == 0x9100_0000 {
            // add xd, xn|sp, #imm[, lsl #12]. Register 31 is the stack pointer here.
            let immediate = ((word >> 10) & 0xfff) as u64;
            let immediate = if word & 0x0040_0000 != 0 {
                immediate << 12
            } else {
                immediate
            };
            let value = match (rn, registers[rn]) {
                (31, _) => Value::Stack(immediate),
                (_, Value::Constant(base)) => Value::Constant(base.wrapping_add(immediate)),
                _ => Value::Unknown,
            };
            if rd != 31 {
                registers[rd] = value;
            }
        } else if word & 0xffe0_ffe0 == 0xaa00_03e0 {
            // mov xd, xm
            registers[rd] = registers[((word >> 16) & 31) as usize];
        } else if word & 0xfc00_0000 == 0x9400_0000 {
            // bl target
            let offset = ((word & 0x03ff_ffff) as i64) << 38 >> 36;
            let target = (address as i64).wrapping_add(offset) as u64;
            if anchors.base_constructors.contains(&target) {
                passed.push(match objects.iter().find(|(at, _)| *at == registers[1]) {
                    Some(&(_, literal)) if registers[1] != Value::Unknown => Passed::Built(literal),
                    _ => Passed::Other(registers[1]),
                });
            } else {
                objects.retain(|(at, _)| *at != registers[0]);
                if anchors.string_constructors.contains(&target)
                    && registers[0] != Value::Unknown
                    && let Value::Constant(literal) = registers[1]
                {
                    objects.push((registers[0], literal));
                    built.push(Built {
                        destination: registers[0],
                        literal,
                    });
                }
            }
            registers[..18].fill(Value::Unknown);
        } else {
            // Any other instruction may write its low register field, and a load or store with
            // writeback also writes its base register.
            let pair_writeback = word & 0x3a00_0000 == 0x2800_0000 && word & 0x0080_0000 != 0;
            let single_writeback = word & 0x3b20_0400 == 0x3800_0400;
            if pair_writeback || single_writeback {
                registers[rn] = Value::Unknown;
            }
            registers[rd] = Value::Unknown;
        }
    }
    (built, passed)
}

fn text(strings: &BTreeMap<u64, String>, literal: u64) -> Option<String> {
    let text = strings.get(&literal)?.trim_end_matches('/');
    (!text.is_empty()).then(|| text.to_owned())
}

/// The argument of every base-constructor call in one constructor body.
pub fn arguments(
    constructor: &Constructor,
    anchors: &Anchors,
    strings: &BTreeMap<u64, String>,
) -> Vec<Argument> {
    scan(constructor, anchors)
        .1
        .into_iter()
        .map(|passed| match passed {
            Passed::Built(literal) => {
                text(strings, literal).map_or(Argument::Unknown, Argument::Literal)
            }
            Passed::Other(Value::Constant(global)) => Argument::Global(global),
            Passed::Other(_) => Argument::Unknown,
        })
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
        for built in scan(initializer, anchors).0 {
            if let (Value::Constant(global), Some(text)) =
                (built.destination, text(strings, built.literal))
            {
                found.entry(global).or_default().insert(text);
            }
        }
    }
    found
        .into_iter()
        .filter(|(_, texts)| texts.len() == 1)
        .map(|(global, mut texts)| (global, texts.pop_first().expect("one text")))
        .collect()
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
    const NOP: u32 = 0xd503_201f;
    // adrp x1, one page after the function at 0x1000.
    const ADRP_X1: u32 = 0xb000_0001;
    const ADD_X0_SP_8: u32 = 0x9100_23e0;
    const ADD_X1_SP_8: u32 = 0x9100_23e1;
    const MOV_X0_X22: u32 = 0xaa16_03e0;

    fn add_x1(immediate: u32) -> u32 {
        0x9100_0021 | immediate << 10
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
        assert!(scan(&call, &anchors()).0.is_empty());
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
}
