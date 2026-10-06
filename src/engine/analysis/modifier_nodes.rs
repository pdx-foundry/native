//! The engine's modifier node graph: each node's source nodes, owner and category mask.
//!
//! A modifier node is a modifier total that adds the totals of its source nodes and keeps only the
//! entries whose categories meet its mask. The node types name their sources, so the graph comes
//! from symbols. Each node is built by one call to a node base constructor in its owner's
//! constructor. The call passes a pointer to the category mask and a closure that holds the node's
//! calculation function.
//!
//! The method runs each owner constructor to that call and reads the mask there: a stack value or a
//! constant. A mask in zero-fill memory comes from the one static initializer that stores it; the
//! method reads the word at every return of that initializer, so a path that skips the store leaves
//! the mask unresolved. Zero-fill memory is never read as zero.
//!
//! A node's calculation function can set the mask again. The method finds each store at the mask's
//! offset in that function's own body, runs the function to it with a scratch node, and keeps the
//! stores that hit the node. Stores through an adjusted node pointer and stores in called functions
//! are not searched.
//!
//! Outside the method: where a modifier takes effect. That depends on each receiver's mask and on
//! the include and exclude masks of each propagation edge, which this method does not read.
use std::collections::{BTreeMap, BTreeSet};

use super::decode::{Instruction, adrp, decode_arm64};
use super::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};
use super::modifiers::{CategoryInput, CategoryNames, EVERY_CATEGORY, category_names};
use super::stop::Unresolved;

#[cfg(test)]
mod tests;

/// Name and revision of the modifier node method.
pub const METHOD: &str = "modifier-nodes/v1";

/// Bytes of scratch memory for an owner or a node. Constructors on M451-hotfix write below 0x1000.
const OBJECT_SPAN: u64 = 0x1_0000;

/// The most paths that one run follows. An owner constructor splits on each conditional select of
/// an unknown comparison and on each loop pass before its node constructor call: the second country
/// node on M451-hotfix needs 1,296 paths.
const PATH_LIMIT: usize = 4096;

/// Where a node keeps its category mask and how a node constructor receives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModifierNodeLayout {
    /// Offset of the 32-bit category mask in a node.
    pub category_offset: u64,
    /// Argument register of a node constructor that points to the category mask.
    pub category_argument: usize,
    /// Argument register of a node constructor that points to the calculation closure.
    pub calculation_argument: usize,
    /// Offset of the calculation function in that closure.
    pub calculation_function_offset: u64,
}

/// One direct call to a node base constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Construction {
    pub node: u32,
    /// The engine type that the constructor's symbol names as the node's owner.
    pub owner: String,
    /// Entry of the function that holds the call.
    pub caller: u64,
    pub site: u64,
}

/// The executable text and the function starts that symbols give it.
#[derive(Debug, Clone, Default)]
pub struct Text {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub starts: BTreeSet<u64>,
}

impl Text {
    /// The bytes from `start` to the next function start, or to the end of the text.
    fn function(&self, start: u64) -> Option<&[u8]> {
        let end = self.address + self.bytes.len() as u64;
        if !(self.address..end).contains(&start) {
            return None;
        }

        let next = self
            .starts
            .range(start + 1..)
            .next()
            .copied()
            .unwrap_or(end);
        let offset = (start - self.address) as usize;
        self.bytes.get(offset..offset + (next - start) as usize)
    }

    fn rows(&self, start: u64) -> Result<Vec<Instruction>, Unresolved> {
        let bytes = self.function(start).ok_or(Unresolved::new("function"))?;
        decode_arm64(bytes, start).map_err(|_| Unresolved::new("decode"))
    }
}

/// Executable-derived input of the modifier node method.
pub struct ModifierNodeInput {
    /// The source nodes of each node that a node type symbol names. `Err` when two symbols give
    /// one node different sources.
    pub sources: BTreeMap<u32, Result<Vec<u32>, Unresolved>>,
    /// Each direct call to a node base constructor once, in address order.
    pub constructions: Vec<Construction>,
    /// Entry of each static initializer.
    pub initializers: Vec<u64>,
    pub text: Text,
    pub data: ReadOnlyData,
    pub layout: ModifierNodeLayout,
    pub categories: CategoryInput,
}

/// The masks that one construction gives its node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Masks {
    /// The constructor's mask; the calculation function stores no other.
    Constant(u64),
    /// The constructor's mask and each mask that the calculation function can store.
    Recalculated(BTreeSet<u64>),
}

/// One owner of a node and the masks that its construction gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerRead {
    pub owner: String,
    pub masks: Result<Masks, Unresolved>,
}

/// One node of the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRead {
    /// `Err` when no node type symbol names the node, or two give it different sources.
    pub sources: Result<Vec<u32>, Unresolved>,
    /// One for each construction, in address order. Empty when the method found none.
    pub owners: Vec<OwnerRead>,
}

/// Every node, and the name of each category mask that a resolved mask uses.
pub struct ModifierNodeResult {
    pub nodes: BTreeMap<u32, NodeRead>,
    pub categories: CategoryNames,
    /// Each 32-bit mask in zero-fill memory that a node constructor reads, with the value that
    /// its static initializer leaves.
    pub initialized_words: BTreeMap<u64, u64>,
}

/// Read every node's sources, owners and masks.
pub fn analyze(input: &ModifierNodeInput) -> ModifierNodeResult {
    let mut nodes: BTreeMap<u32, NodeRead> = input
        .sources
        .iter()
        .map(|(&node, sources)| {
            let read = NodeRead {
                sources: sources.clone(),
                owners: Vec::new(),
            };
            (node, read)
        })
        .collect();

    let mut initialized_words = BTreeMap::new();
    for construction in &input.constructions {
        let owner = OwnerRead {
            owner: construction.owner.clone(),
            masks: construction_masks(input, construction, &mut initialized_words),
        };
        nodes
            .entry(construction.node)
            .or_insert_with(|| NodeRead {
                sources: Err(Unresolved::new("node-type")),
                owners: Vec::new(),
            })
            .owners
            .push(owner);
    }

    let masks = nodes
        .values()
        .flat_map(|node| &node.owners)
        .filter_map(|owner| owner.masks.as_ref().ok())
        .flat_map(|masks| match masks {
            Masks::Constant(mask) => vec![*mask],
            Masks::Recalculated(masks) => masks.iter().copied().collect(),
        });
    let categories = category_names(&input.categories, masks.collect::<Vec<_>>());

    ModifierNodeResult {
        nodes,
        categories,
        initialized_words,
    }
}

/// The constructor's mask, and the masks that the node's calculation function stores. A mask
/// read from zero-fill memory joins `initialized_words`.
fn construction_masks(
    input: &ModifierNodeInput,
    construction: &Construction,
    initialized_words: &mut BTreeMap<u64, u64>,
) -> Result<Masks, Unresolved> {
    let (mask, calculation) = construction_arguments(input, construction)?;
    let mask = match mask {
        Mask::Known(mask) => mask,
        Mask::Initialized(address) => {
            let mask = initialized_word(input, address)?;
            initialized_words.insert(address, mask);
            mask
        }
    };
    let mut masks = calculation_masks(input, calculation)?;
    masks.insert(mask);

    if masks.len() == 1 {
        return Ok(Masks::Constant(mask));
    }
    Ok(Masks::Recalculated(masks))
}

/// Where the category mask is, and the calculation function, that one constructor call passes.
fn construction_arguments(
    input: &ModifierNodeInput,
    construction: &Construction,
) -> Result<(Mask, u64), Unresolved> {
    let layout = input.layout;
    let code = Code::from_rows(input.text.rows(construction.caller)?);
    let mut machine = Machine::new(&code, &input.data);
    let owner = machine.reserve(OBJECT_SPAN);
    machine.set_register(0, owner);
    machine.set_path_limit(PATH_LIMIT);

    let paths = machine.run_paths_to(construction.caller, construction.site, &mut |_, _| {
        Ok(Call::Return(None))
    });
    let mut arguments = BTreeSet::new();

    for path in paths {
        match path.end? {
            Exit::Reached => {}
            Exit::Trapped => continue,
            _ => return Err(Unresolved::new("construction-path")),
        }

        let machine = path.machine;
        let mask_address = machine.known_register(layout.category_argument, "category-mask")?;
        let mask = match machine.read(mask_address, 4) {
            Some(mask) => Mask::Known(mask),
            None if machine.is_stack(mask_address) => {
                return Err(Unresolved::new("category-mask"));
            }
            None => Mask::Initialized(mask_address),
        };
        let closure = machine.known_register(layout.calculation_argument, "calculation")?;
        let calculation = machine
            .read(closure + layout.calculation_function_offset, 8)
            .ok_or(Unresolved::new("calculation"))?;

        arguments.insert((mask, calculation));
    }

    match (arguments.first(), arguments.len()) {
        (Some(&arguments), 1) => Ok(arguments),
        (_, 0) => Err(Unresolved::new("construction-path")),
        _ => Err(Unresolved::new("construction-arguments")),
    }
}

/// Where a constructor call finds its category mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Mask {
    Known(u64),
    /// A word that the executable does not hold, such as one in zero-fill memory.
    Initialized(u64),
}

/// The 32-bit value that the one static initializer which stores to `address` leaves there.
fn initialized_word(input: &ModifierNodeInput, address: u64) -> Result<u64, Unresolved> {
    let mut values = Vec::new();

    for &initializer in &input.initializers {
        let Some(bytes) = input.text.function(initializer) else {
            continue;
        };
        let page_registers = page_registers(bytes, initializer, address & !0xfff);
        if page_registers.is_empty() {
            continue;
        }

        let rows = input.text.rows(initializer)?;
        let stores_word = rows.iter().filter_map(store).any(|(_, base, offset)| {
            offset == address & 0xfff
                && register_number(base).is_some_and(|base| page_registers.contains(&base))
        });
        if !stores_word {
            continue;
        }

        let code = Code::from_rows(rows);
        let mut machine = Machine::new(&code, &input.data);
        machine.set_path_limit(PATH_LIMIT);
        let paths = machine.run_paths(initializer, &mut |_, _| Ok(Call::Return(None)));
        let mut wrote = false;
        let mut words = BTreeSet::new();

        for path in paths {
            let end = path.end?;
            if end == Exit::Trapped {
                continue;
            }
            if end != Exit::Returned {
                return Err(Unresolved::new("initializer-path"));
            }

            wrote |= path.machine.has_written(address, 4);
            words.insert(path.machine.read(address, 4));
        }

        if wrote {
            values.push(words);
        }
    }

    let [words] = values.as_slice() else {
        return Err(Unresolved::new("initializer"));
    };
    match (words.first(), words.len()) {
        (Some(Some(word)), 1) => Ok(*word),
        _ => Err(Unresolved::new("initializer-value")),
    }
}

/// The registers into which the function in `bytes` loads the address of `page` with `adrp`.
fn page_registers(bytes: &[u8], start: u64, page: u64) -> BTreeSet<usize> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .zip((start..).step_by(4))
        .filter_map(|(word, at)| adrp(u32::from_le_bytes(*word), at))
        .filter(|&(_, to)| to == page)
        .map(|(register, _)| register)
        .collect()
}

/// The number of a 64-bit general register, such as 19 for `x19`.
fn register_number(name: &str) -> Option<usize> {
    name.strip_prefix('x')?.parse().ok()
}

/// Each mask that the calculation function at `entry` stores to its node's mask.
fn calculation_masks(input: &ModifierNodeInput, entry: u64) -> Result<BTreeSet<u64>, Unresolved> {
    let layout = input.layout;
    let rows = input.text.rows(entry)?;
    let candidates: Vec<_> = rows
        .iter()
        .filter_map(|row| mask_store(row, layout.category_offset))
        .collect();
    let code = Code::from_rows(rows);
    let mut masks = BTreeSet::new();

    for (site, value, base) in candidates {
        let mut machine = Machine::new(&code, &input.data);
        let owner = machine.reserve(OBJECT_SPAN);
        let node = machine.reserve(OBJECT_SPAN);
        machine.set_register(0, owner);
        machine.set_register(1, node);
        machine.set_path_limit(PATH_LIMIT);

        let paths = machine.run_paths_to(entry, site, &mut |_, _| Ok(Call::Return(None)));

        for path in paths {
            match path.end? {
                Exit::Reached => {}
                Exit::Returned | Exit::Trapped => continue,
                _ => return Err(Unresolved::new("calculation-path")),
            }

            let machine = path.machine;
            let target = machine.known_register(base, "mask-store")? + layout.category_offset;
            if target != node + layout.category_offset {
                continue;
            }

            let mask = match value {
                Some(register) => machine.known_register(register, "stored-mask")?,
                None => 0,
            };
            masks.insert(mask & EVERY_CATEGORY);
        }
    }

    Ok(masks)
}

/// A store with an immediate offset from one register: the stored register, the base register and
/// the offset.
fn store(row: &Instruction) -> Option<(&str, &str, u64)> {
    if !matches!(row.operation.as_str(), "str" | "stur") {
        return None;
    }

    let (value, memory) = row.operands.split_once(",[")?;
    let (base, offset) = memory.strip_suffix(']')?.split_once(",#")?;
    let offset = u64::from_str_radix(offset.strip_prefix("0x")?, 16).ok()?;
    Some((value, base, offset))
}

/// A 32-bit store at `offset` from a general register: its address, the stored register (`None`
/// for the zero register) and the base register.
fn mask_store(row: &Instruction, offset: u64) -> Option<(u64, Option<usize>, usize)> {
    let (value, base, displacement) = store(row)?;
    if displacement != offset {
        return None;
    }

    let value = match value {
        "wzr" => None,
        _ => Some(value.strip_prefix('w')?.parse().ok()?),
    };
    Some((row.address, value, register_number(base)?))
}
