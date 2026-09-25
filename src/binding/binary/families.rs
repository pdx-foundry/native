//! Read the inputs of the modifier-family method from executable text.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::engine::analysis::{
    decode::{Instruction, add_immediate, adrp, decode_arm64},
    discovery::Symbol,
    evaluate::{Code, ReadOnlyData},
    families::{
        DatabaseLayout, Definitions, FamilyInput, KeyStorageInput, Receiver, RegistryInput, Root,
        StringFunctions, StringLayout,
        joins::{self, DEPTH, GenerationSite, Graph, Joins, RootOf},
        loading::{Function, Loading},
    },
    modifier_table::Layout,
};

use super::super::targets::DeclarationRecipe;
use super::declarations::{Text, addresses, call_or_jump_target, unique};
use super::language::{GENERATE_MODIFIER, constant_data, generation_calls};

/// Import stubs keep their raw name when it does not demangle.
const NEVER_RETURN: [&str; 5] = [
    "___stack_chk_fail",
    "__Unwind_Resume",
    "___cxa_throw",
    "___cxa_rethrow",
    "_abort",
];

const ASSIGN_TEXT: &str = "std::__1::basic_string<char, std::__1::char_traits<char>, CPdxCommonStringAllocator>::__assign_external(char const*, unsigned long)";

const DEFINITIONS: &str = "CPdxModifier<ModifierType, ModifierCategory, CModifier, CDefaultPdxModifierValueReader>::_Definitions";

/// The post-read functions that the engine runs for a content object.
const POST_READ: [&str; 2] = ["::InitPostRead(", "::PostReadInit()"];

/// The functions that return new memory.
const ALLOCATIONS: [&str; 3] = ["_calloc", "_malloc", "operator new(unsigned long)"];

/// A named registry and the classes of its database and its items.
pub(in crate::binding) struct NamedRegistry<'a> {
    pub name: &'a str,
    pub database: &'a str,
    pub owner: &'a str,
}

/// Facts from the rest of the static analysis that the family input needs.
pub(in crate::binding) struct KnownFacts<'a> {
    /// The addresses of the direct definition calls whose token is composed at run time.
    pub runtime_definitions: &'a [u64],
    /// The category mask of each modifier type that a direct definition declares.
    pub type_masks: BTreeMap<u64, u64>,
    /// The loaded modifier table's layout, when it is established.
    pub table: Option<Layout>,
    pub pointers: &'a BTreeMap<u64, u64>,
    pub bound_slots: &'a BTreeSet<u64>,
}

/// Every generation call, its joins, and the code of each named registry.
pub(crate) struct FamilyIndex {
    pub input: FamilyInput,
    pub joins: Joins,
    /// Each named registry, with no root when none of its code reaches a generation call.
    pub registries: BTreeMap<String, RegistryInput>,
}

/// Read the generation calls, join them to the named registries, and read each registry's code.
pub(in crate::binding) fn index(
    bytes: &[u8],
    symbols: &[Symbol],
    registries: &[NamedRegistry],
    facts: KnownFacts,
    recipe: &DeclarationRecipe,
    database: DatabaseLayout,
) -> Result<FamilyIndex, AnalysisError> {
    let text = Text::read(bytes, symbols)?;
    let names = Names::new(symbols);
    let registration = unique(symbols, GENERATE_MODIFIER[0])?;
    let strings = string_functions(symbols);

    let graph = graph(&text, symbols, &names, registries, registration, &facts)?;
    let joins = joins::join(&graph);
    let data = constant_data(bytes, facts.pointers, facts.bound_slots)?;

    let mut inputs = BTreeMap::new();
    for registry in registries {
        let join = joins.registries.get(registry.name);
        let roots: Vec<Root> = join
            .into_iter()
            .flat_map(|join| &join.roots)
            .map(|(&function, (receiver, sites))| Root {
                function,
                receiver: *receiver,
                sites: sites.clone(),
            })
            .collect();
        let path = join.map(|join| join.path.clone()).unwrap_or_default();
        let entered = entered(&text, &names, &strings, &roots, path, registration)?;
        let constructors = constructors(symbols, registry.owner);
        let loading = roots
            .iter()
            .any(|root| root.receiver == Receiver::Item)
            .then(|| loading(&text, symbols, &names, &data, registry, &roots));

        let functions: Vec<u64> = roots
            .iter()
            .map(|root| root.function)
            .chain(entered.iter().copied())
            .chain(constructors.iter().copied())
            .collect();
        inputs.insert(
            registry.name.to_owned(),
            RegistryInput {
                roots,
                entered,
                constructors,
                loading,
                code: decoded(&text, &functions),
            },
        );
    }

    let definitions = match facts.table {
        Some(layout) => Some(Definitions {
            table: unique(symbols, DEFINITIONS)?,
            data_offset: layout.array_data_offset,
            stride: layout.definition_stride,
            mask_offset: layout.mask_offset,
            masks: facts.type_masks.clone(),
        }),
        None => None,
    };

    Ok(FamilyIndex {
        input: FamilyInput {
            registration,
            category_offset: recipe.dynamic_modifier_category_offset,
            database,
            definitions,
            strings,
            layout: StringLayout {
                flag_byte: recipe.short_string_length_offset,
            },
            data,
        },
        joins,
        registries: inputs,
    })
}

/// The generation calls, their callers up to [`DEPTH`] calls away, and every root.
fn graph(
    text: &Text,
    symbols: &[Symbol],
    names: &Names,
    registries: &[NamedRegistry],
    registration: u64,
    facts: &KnownFacts,
) -> Result<Graph, AnalysisError> {
    let containing = |address: u64| text.starts.range(..=address).next_back().copied();
    let calls = generation_calls(text, symbols)?
        .into_iter()
        .chain(facts.runtime_definitions.iter().copied());

    let mut sites = Vec::new();
    for call in calls {
        let function = containing(call).ok_or(AnalysisError::InvalidRange)?;
        sites.push(GenerationSite {
            call,
            function,
            inside_registration: function == registration,
        });
    }
    sites.sort_by_key(|site| site.call);

    let roots = roots(symbols, registries);
    let mut callers: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    let mut level: BTreeSet<u64> = sites
        .iter()
        .map(|site| site.function)
        .filter(|function| !roots.contains_key(function))
        .collect();
    let mut seen = level.clone();
    for _ in 0..DEPTH {
        let mut next = BTreeSet::new();
        for (at, target) in text.calls_into(&level) {
            let Some(caller) = containing(at) else {
                continue;
            };
            callers.entry(target).or_default().insert(caller);
            if !roots.contains_key(&caller) && seen.insert(caller) {
                next.insert(caller);
            }
        }
        level = next;
    }

    let unnamed_classes: BTreeSet<&str> = roots
        .iter()
        .filter(|(_, owner)| **owner == RootOf::UnnamedContent)
        .filter_map(|(function, _)| names.of(*function).find_map(post_read_class))
        .collect();
    let graph_functions = callers
        .iter()
        .flat_map(|(callee, callers)| callers.iter().chain([callee]))
        .copied();
    let unnamed_inputs = graph_functions
        .filter(|&function| {
            names
                .of(function)
                .any(|name| takes_any(name, &unnamed_classes))
        })
        .collect();

    Ok(Graph {
        sites,
        callers,
        roots,
        unnamed_inputs,
    })
}

/// Each named registry's database generators, database post-read loops and item post-read
/// functions, and the post-read function of every other content class.
fn roots(symbols: &[Symbol], registries: &[NamedRegistry]) -> BTreeMap<u64, RootOf> {
    let mut roots = BTreeMap::new();
    for registry in registries {
        let loop_prefix = format!(
            "TSingleObjectGameDatabase<{}, {}, ",
            registry.database, registry.owner
        );
        let item_prefix = format!("{}::InitPostRead(", registry.owner);
        let generator = format!("{}::GenerateModifiers()", registry.database);

        for symbol in symbols {
            let name = symbol.name.as_str();
            let receiver = if name == generator
                || (name.starts_with(&loop_prefix) && name.ends_with(">::PostReadInit()"))
            {
                Receiver::Database
            } else if name.starts_with(&item_prefix) && !name.contains(".cold.") {
                Receiver::Item
            } else {
                continue;
            };
            roots
                .entry(symbol.address)
                .or_insert_with(|| RootOf::Registry {
                    registry: registry.name.into(),
                    receiver,
                });
        }
    }

    let named = |class: &str| {
        registries.iter().any(|registry| {
            class == registry.owner
                || class == registry.database
                || class.contains(&format!("<{},", registry.database))
                || class.contains(&format!("<{}>", registry.database))
        })
    };
    for symbol in symbols {
        let Some(class) = post_read_class(&symbol.name) else {
            continue;
        };
        if !named(class) {
            roots
                .entry(symbol.address)
                .or_insert(RootOf::UnnamedContent);
        }
    }

    roots
}

/// The class of a post-read function, when `name` is one. Thunks and cold parts are not.
fn post_read_class(name: &str) -> Option<&str> {
    if name.contains("thunk") || name.contains(".cold.") {
        return None;
    }
    POST_READ
        .iter()
        .find_map(|suffix| name.find(suffix).map(|end| &name[..end]))
}

/// Whether the parameters of the function `name` take an object of one of `classes`.
fn takes_any(name: &str, classes: &BTreeSet<&str>) -> bool {
    let Some(parameters) = name.find('(').map(|start| &name[start..]) else {
        return false;
    };
    classes.iter().any(|class| {
        ["(", ", "].iter().any(|before| {
            [" const&", "&", " const*", "*"]
                .iter()
                .any(|after| parameters.contains(&format!("{before}{class}{after}")))
        })
    })
}

/// The functions that a run of the registry enters: the functions between its roots and a
/// generation call, and the composers that they or the roots call.
fn entered(
    text: &Text,
    names: &Names,
    strings: &StringFunctions,
    roots: &[Root],
    path: BTreeSet<u64>,
    registration: u64,
) -> Result<BTreeSet<u64>, AnalysisError> {
    let callers: BTreeSet<u64> = roots
        .iter()
        .map(|root| root.function)
        .chain(path.iter().copied())
        .collect();
    let mut entered = path;
    for &caller in &callers {
        for call in calls(text, caller)? {
            let Call::Direct(callee) = call else {
                continue;
            };
            let modelled = strings.follows(callee) || strings.never_return.contains(&callee);
            if modelled
                || callee == registration
                || callers.contains(&callee)
                || !text.starts.contains(&callee)
            {
                continue;
            }
            let kinds: Vec<CallKind> = calls(text, callee)?
                .into_iter()
                .map(|call| call_kind(names, strings, callee, call))
                .collect();
            if is_composer(&kinds) {
                entered.insert(callee);
            }
        }
    }
    Ok(entered)
}

/// One call or tail call out of a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    Direct(u64),
    /// A call through a register.
    Indirect,
}

/// The code that constructs the registry's items: the functions that call a constructor of the
/// item's class, directly or through a constructor that delegates, the database constructor,
/// and the item's vtables.
fn loading(
    text: &Text,
    symbols: &[Symbol],
    names: &Names,
    data: &ReadOnlyData,
    registry: &NamedRegistry,
    roots: &[Root],
) -> Loading {
    let constructor_prefix = format!("{0}::{0}(", registry.owner);
    let constructors: BTreeSet<u64> = symbols
        .iter()
        .filter(|symbol| {
            symbol.name.starts_with(&constructor_prefix) && !symbol.name.contains(".cold.")
        })
        .map(|symbol| symbol.address)
        .collect();
    let (loaders, elsewhere) = constructing_functions(text, names, registry, &constructors);

    let database_constructor = format!("{0}::{0}()", registry.database);
    let database_constructors: Vec<Function> = addresses(symbols, &database_constructor)
        .into_iter()
        .map(|address| function(address, &database_constructor))
        .collect();

    let item_roots: BTreeSet<u64> = roots
        .iter()
        .filter(|root| root.receiver == Receiver::Item)
        .map(|root| root.function)
        .collect();
    let group = vtable_group(symbols, data, registry.owner).unwrap_or_default();
    let thunks: BTreeMap<u64, (u64, u64)> = thunks(symbols, names, &item_roots)
        .into_iter()
        .filter_map(|(thunk, root)| match group.slots.get(&thunk) {
            Some(offsets) if offsets.len() == 1 => Some((thunk, (root, *offsets.first()?))),
            _ => None,
        })
        .collect();
    let dispatch: BTreeSet<u64> = group
        .slots
        .keys()
        .copied()
        .filter(|slot| {
            text.starts.contains(slot) && !item_roots.contains(slot) && !thunks.contains_key(slot)
        })
        .collect();

    let destructor_prefix = format!("{0}::~{0}(", registry.owner);
    let own_code = |function: u64| {
        constructors.contains(&function)
            || names.of(function).any(|name| {
                name.starts_with(&destructor_prefix)
                    || name.starts_with(&format!("non-virtual thunk to {destructor_prefix}"))
                    || name.starts_with(&constructor_prefix)
            })
    };
    let points = group.address_points.values().copied().collect();
    let inline_constructions = functions_forming(text, &points)
        .into_iter()
        .filter(|&function| !own_code(function))
        .map(|function| {
            names
                .of(function)
                .next()
                .map_or_else(|| format!("{function:#x}"), str::to_owned)
        })
        .collect();

    let functions: Vec<u64> = loaders
        .iter()
        .chain(&database_constructors)
        .map(|function| function.address)
        .chain(dispatch.iter().copied())
        .collect();
    Loading {
        loaders,
        database_constructors,
        elsewhere,
        inline_constructions,
        constructors,
        vtables: group.address_points,
        thunks,
        dispatch,
        allocations: ALLOCATIONS
            .iter()
            .flat_map(|name| addresses(symbols, name))
            .collect(),
        code: decoded(text, &functions),
    }
}

/// The functions whose code forms one of `points` in a register. Each function with an `adrp`
/// of a point's page is decoded and read in address order: `adrp` gives a page, `add` of an
/// immediate and `mov` carry a known value, and every other write makes the register unknown. A
/// function that does not decode is read as words, with `adrp` and `add` only, and a register
/// keeps its value through other writes. An address that code forms in another way, such as
/// across branches in another order, is not found.
fn functions_forming(text: &Text, points: &BTreeSet<u64>) -> BTreeSet<u64> {
    let pages: BTreeSet<u64> = points.iter().map(|point| point & !0xfff).collect();
    let candidates: BTreeSet<u64> = text
        .code
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter_map(|(index, word)| {
            let at = text.address + index as u64 * 4;
            let (_, page) = adrp(u32::from_le_bytes(*word), at)?;
            pages.contains(&page).then_some(at)
        })
        .filter_map(|at| text.starts.range(..=at).next_back().copied())
        .collect();

    candidates
        .into_iter()
        .filter(|&function| {
            let Ok((address, code)) = text.function(function) else {
                return true;
            };
            match decode_arm64(code, address) {
                Ok(rows) => forms(&rows, points),
                Err(_) => words_form(code, address, points),
            }
        })
        .collect()
}

/// Whether the words at `address`, in address order, put one of `points` in a register through
/// `adrp` and `add` of an immediate.
fn words_form(code: &[u8], address: u64, points: &BTreeSet<u64>) -> bool {
    let mut values: BTreeMap<usize, u64> = BTreeMap::new();
    for (index, word) in code.as_chunks::<4>().0.iter().enumerate() {
        let word = u32::from_le_bytes(*word);
        if let Some((destination, page)) = adrp(word, address + index as u64 * 4) {
            values.insert(destination, page);
        } else if let Some((destination, source, addend)) = add_immediate(word)
            && let Some(&value) = values.get(&source)
        {
            let sum = value.wrapping_add(addend);
            if points.contains(&sum) {
                return true;
            }
            values.insert(destination, sum);
        }
    }
    false
}

/// Whether the instructions, in address order, put one of `points` in a register.
fn forms(rows: &[Instruction], points: &BTreeSet<u64>) -> bool {
    let mut values: BTreeMap<usize, u64> = BTreeMap::new();
    for row in rows {
        let operands: Vec<&str> = row.operands.split(',').collect();
        let value = match (row.operation.as_str(), operands.as_slice()) {
            ("adrp", [_, page]) => parse_immediate(page),
            ("add", [_, source, addend, shift @ ..]) if matches!(shift, [] | ["lsl#12"]) => {
                let source_value = register(source).and_then(|source| values.get(&source));
                let addend = parse_immediate(addend).map(|addend| addend << (12 * shift.len()));
                source_value
                    .zip(addend)
                    .map(|(value, addend)| value.wrapping_add(addend))
            }
            ("mov", [_, source]) => {
                register(source).and_then(|source| values.get(&source).copied())
            }
            _ => None,
        };
        if value.is_some_and(|value| points.contains(&value)) {
            return true;
        }
        for written in written_registers(&row.operation, &operands) {
            values.remove(&written);
        }
        if let (Some(value), Some(destination)) =
            (value, operands.first().and_then(|o| register(o)))
        {
            values.insert(destination, value);
        }
    }
    false
}

/// The general registers that an instruction writes, as far as the scan needs them: a call
/// writes the caller-saved registers and the link register.
pub(in crate::binding) fn written_registers(operation: &str, operands: &[&str]) -> Vec<usize> {
    let calls = ["bl", "blr"];
    let writes_nothing = operation.starts_with("st")
        || operation.starts_with("b")
        || operation.starts_with("cb")
        || operation.starts_with("tb")
        || [
            "cmp", "cmn", "tst", "ccmp", "ccmn", "fcmp", "ret", "nop", "prfm",
        ]
        .contains(&operation);
    if calls.contains(&operation) {
        return (0..=18).chain([30]).collect();
    }
    if writes_nothing {
        return Vec::new();
    }
    let count = if operation.starts_with("ldp") || operation.starts_with("ldnp") {
        2
    } else {
        1
    };
    operands
        .iter()
        .take(count)
        .filter_map(|operand| register(operand))
        .collect()
}

/// The number of general register `name`, such as `x8` or `w8`.
pub(in crate::binding) fn register(name: &str) -> Option<usize> {
    name.strip_prefix('x')
        .or_else(|| name.strip_prefix('w'))?
        .parse()
        .ok()
}

/// An immediate operand such as `#0x10`.
fn parse_immediate(operand: &str) -> Option<u64> {
    let text = operand.strip_prefix('#')?;
    match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

/// The functions other than `constructors` that call one of them: those of the database's own
/// classes, and the names of the others, except the null object's initializer.
fn constructing_functions(
    text: &Text,
    names: &Names,
    registry: &NamedRegistry,
    constructors: &BTreeSet<u64>,
) -> (Vec<Function>, Vec<String>) {
    let own_classes = [
        format!("{}::", registry.database),
        format!(
            "TSingleObjectGameDatabase<{}, {}, ",
            registry.database, registry.owner
        ),
    ];
    let null_object = format!("TPdxNullObject<{}>::Initialize()", registry.owner);
    let containing = |address: u64| text.starts.range(..=address).next_back().copied();

    let mut loaders = BTreeMap::new();
    let mut elsewhere = BTreeSet::new();
    for (at, _) in text.calls_into(constructors) {
        let Some(caller) = containing(at) else {
            continue;
        };
        if constructors.contains(&caller) {
            continue;
        }
        let name = names
            .of(caller)
            .find(|name| !name.contains("thunk"))
            .unwrap_or_default();
        if own_classes.iter().any(|class| name.starts_with(class)) {
            loaders.insert(caller, function(caller, name));
        } else if name != null_object && !is_cold_part_of(names, caller, constructors) {
            elsewhere.insert(if name.is_empty() {
                format!("{caller:#x}")
            } else {
                name.to_owned()
            });
        }
    }
    (
        loaders.into_values().collect(),
        elsewhere.into_iter().collect(),
    )
}

/// A function that a run of the loading code starts at.
fn function(address: u64, name: &str) -> Function {
    Function {
        address,
        name: name.to_owned(),
        pointers: pointer_arguments(name),
    }
}

/// The thunks of each root in `roots`: `non-virtual thunk to <root>` and `{virtual override
/// thunk(…, <root>)}`. A thunk adjusts `this` and runs its root; the compiler may give it a copy
/// of the root's body instead of a branch to the root.
fn thunks(symbols: &[Symbol], names: &Names, roots: &BTreeSet<u64>) -> BTreeMap<u64, u64> {
    let mut thunks = BTreeMap::new();
    for &root in roots {
        for name in names.of(root) {
            let non_virtual = format!("non-virtual thunk to {name}");
            let virtual_override = format!(", {name})}}");
            for symbol in symbols {
                let thunk = symbol.name == non_virtual
                    || (symbol.name.starts_with("{virtual override thunk(")
                        && symbol.name.ends_with(&virtual_override));
                if thunk {
                    thunks.insert(symbol.address, root);
                }
            }
        }
    }
    thunks
}

/// Whether `function` is an out-of-line part of one of `functions`, which only unwinding
/// reaches.
fn is_cold_part_of(names: &Names, function: u64, functions: &BTreeSet<u64>) -> bool {
    names.of(function).any(|cold| {
        functions
            .iter()
            .any(|&other| names.of(other).any(|name| is_cold_part(cold, name)))
    })
}

/// Whether the symbol `cold` names an out-of-line part of the function `name`.
fn is_cold_part(cold: &str, name: &str) -> bool {
    cold.strip_prefix(name)
        .is_some_and(|suffix| suffix.contains(".cold."))
}

/// The argument registers of the demangled member function `name` that hold a pointer or a
/// reference. `x0` holds `this`, so the parameters start at `x1`.
fn pointer_arguments(name: &str) -> Vec<usize> {
    let Some(parameters) = name
        .find('(')
        .zip(name.rfind(')'))
        .and_then(|(start, end)| name.get(start + 1..end))
    else {
        return Vec::new();
    };
    let mut depth = 0;
    let mut parts = vec![String::new()];
    for character in parameters.chars() {
        match character {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(String::new());
                continue;
            }
            _ => {}
        }
        parts.last_mut().expect("one part").push(character);
    }
    parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .enumerate()
        .filter(|(_, part)| part.ends_with('&') || part.ends_with('*'))
        .map(|(index, _)| index + 1)
        .collect()
}

/// A class's Itanium vtable group.
#[derive(Debug, Default, PartialEq, Eq)]
struct VtableGroup {
    /// The address point of each vtable, by the offset of its subobject in the object.
    address_points: BTreeMap<u64, u64>,
    /// Every known address in the group's slots, with the subobject offset of each vtable that
    /// holds it.
    slots: BTreeMap<u64, BTreeSet<u64>>,
}

/// Read the vtable group of `class` from `vtable for <class>` up to the next symbol. Each vtable
/// starts with its offset to the top of the object, zero or negative, and `typeinfo for
/// <class>`; its address point follows them. The other words are slots.
fn vtable_group(symbols: &[Symbol], data: &ReadOnlyData, class: &str) -> Option<VtableGroup> {
    let start = unique(symbols, &format!("vtable for {class}")).ok()?;
    let typeinfo = unique(symbols, &format!("typeinfo for {class}")).ok()?;
    let end = symbols
        .iter()
        .map(|symbol| symbol.address)
        .filter(|&address| address > start)
        .min()?;

    let mut group = VtableGroup::default();
    let mut subobject = None;
    let mut at = start;
    while at + 8 <= end {
        if let (Some(offset), Some(info)) = (data.read(at, 8), data.read(at + 8, 8))
            && at + 16 <= end
            && info == typeinfo
            && (offset as i64) <= 0
        {
            let offset = (offset as i64).unsigned_abs();
            group.address_points.insert(offset, at + 16);
            subobject = Some(offset);
            at += 16;
            continue;
        }
        if let (Some(offset), Some(slot)) = (subobject, data.read(at, 8)) {
            group.slots.entry(slot).or_default().insert(offset);
        }
        at += 8;
    }
    group.address_points.contains_key(&0).then_some(group)
}

/// Every call and every branch out of the function at `start`.
fn calls(text: &Text, start: u64) -> Result<Vec<Call>, AnalysisError> {
    let (address, code) = text.function(start)?;
    let end = address + code.len() as u64;
    Ok(code
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter_map(|(index, word)| {
            let at = address + (index * 4) as u64;
            let word = u32::from_le_bytes(*word);
            if word & 0xffff_fc1f == 0xd63f_0000 {
                return Some(Call::Indirect);
            }
            let target = call_or_jump_target(word, at)?;
            (!(address..end).contains(&target)).then_some(Call::Direct(target))
        })
        .collect())
}

/// What a call out of a candidate composer goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallKind {
    /// A string function that composes text: a constructor from text, an append, a reserve or
    /// a formatter.
    Composing,
    /// Another function that the model follows: memory, length and copy functions.
    Modelled,
    /// The function's own out-of-line part, which only unwinding reaches.
    OwnColdPart,
    NeverReturns,
    Other,
}

fn call_kind(names: &Names, strings: &StringFunctions, function: u64, call: Call) -> CallKind {
    let Call::Direct(target) = call else {
        return CallKind::Other;
    };
    if strings.never_return.contains(&target) {
        return CallKind::NeverReturns;
    }
    if strings.composes(target) {
        return CallKind::Composing;
    }
    if strings.follows(target) {
        return CallKind::Modelled;
    }
    let own_cold = names
        .of(target)
        .any(|cold| names.of(function).any(|name| is_cold_part(cold, name)));
    if own_cold {
        CallKind::OwnColdPart
    } else {
        CallKind::Other
    }
}

/// A composer is a leaf, or composes text with the string functions. It calls nothing that the
/// model does not follow, other than its own out-of-line parts and functions that never return.
/// A function that only calls memory and copy functions, such as the standard string's own
/// assignment, is not a composer: it writes through the object's buffer pointer, which is often
/// unknown, and such a store makes every known byte unknown.
fn is_composer(calls: &[CallKind]) -> bool {
    let followed = calls.iter().all(|kind| *kind != CallKind::Other);
    let leaf = calls
        .iter()
        .all(|kind| matches!(kind, CallKind::OwnColdPart | CallKind::NeverReturns));
    followed && (leaf || calls.contains(&CallKind::Composing))
}

/// Decode each function on its own and keep the ones that decode completely. A run that
/// reaches a function left out is unresolved there.
fn decoded(text: &Text, functions: &[u64]) -> Code {
    let mut rows = Vec::new();
    for start in functions.iter().collect::<BTreeSet<_>>() {
        let Ok((address, code)) = text.function(*start) else {
            continue;
        };
        if let Ok(function) = decode_arm64(code, address) {
            rows.extend(function);
        }
    }
    Code::from_rows(rows)
}

/// Every demangled name of each address.
struct Names<'a> {
    by_address: BTreeMap<u64, Vec<&'a str>>,
}

impl<'a> Names<'a> {
    fn new(symbols: &'a [Symbol]) -> Self {
        let mut by_address: BTreeMap<u64, Vec<&str>> = BTreeMap::new();
        for symbol in symbols {
            by_address
                .entry(symbol.address)
                .or_default()
                .push(&symbol.name);
        }
        Self { by_address }
    }

    fn of(&self, address: u64) -> impl Iterator<Item = &'a str> + '_ {
        self.by_address.get(&address).into_iter().flatten().copied()
    }
}

/// The item constructors that take the key: by reference or by value.
fn constructors(symbols: &[Symbol], owner: &str) -> Vec<u64> {
    ["CString const&", "CString"]
        .iter()
        .flat_map(|key| addresses(symbols, &format!("{owner}::{owner}(int, {key})")))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Read only the item constructors needed by live registry key reads.
pub(in crate::binding) fn key_storage(
    bytes: &[u8],
    symbols: &[Symbol],
    owner: &str,
    string_tag_offset: u64,
) -> Result<KeyStorageInput, AnalysisError> {
    let constructors = constructors(symbols, owner);
    let text = Text::read(bytes, symbols)?;

    Ok(KeyStorageInput {
        code: decoded(&text, &constructors),
        data: super::declarations::read_only_data(bytes)?,
        strings: string_functions(symbols),
        layout: StringLayout {
            flag_byte: string_tag_offset,
        },
        constructors,
    })
}

pub(super) fn string_functions(symbols: &[Symbol]) -> StringFunctions {
    let named = |name: &str| addresses(symbols, name);
    let formatters: BTreeMap<u64, u64> = symbols
        .iter()
        .filter_map(|symbol| {
            let capacity = symbol
                .name
                .strip_prefix("PdxStrFmt<")?
                .split_once(">::PdxStrFmt(char const*, ...)")
                .filter(|(_, rest)| rest.is_empty())?
                .0
                .parse()
                .ok()?;
            Some((symbol.address, capacity))
        })
        .collect();
    let never_return = symbols
        .iter()
        .filter(|symbol| {
            NEVER_RETURN.contains(&symbol.name.as_str()) || symbol.name.contains("::__throw_")
        })
        .map(|symbol| symbol.address)
        .collect();

    StringFunctions {
        from_text: named("CString::CString(char const*)"),
        append_string: named("CString::operator+=(CString const&)"),
        append_text: named("CString::operator+=(char const*)"),
        append_view: named("CString::operator+=(CPdxStringView)"),
        append_character: named("CString::operator+=(char)"),
        reserves: named("CString::Reserve(unsigned int)"),
        assigns: named(ASSIGN_TEXT),
        formatters,
        allocators: named("CPdxCommonStringAllocator::allocate(unsigned long, void const*)"),
        array_allocators: named("operator new[](unsigned long)"),
        releases: named("CPdxCommonStringAllocator::deallocate(char*, unsigned long)"),
        lengths: named("_strlen"),
        copies: named("_memmove")
            .union(&named("_memcpy"))
            .copied()
            .collect(),
        never_return,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(address: u64, name: &str) -> Symbol {
        Symbol {
            address,
            name: name.into(),
        }
    }

    #[test]
    fn a_vtable_group_gives_each_subobject_its_address_point() {
        const TYPEINFO: u64 = 0x9000;
        let mut bytes = Vec::new();
        for word in [
            0,
            TYPEINFO,
            0x100,
            0x104,
            (-0x38i64) as u64,
            TYPEINFO,
            0x200,
            0x204,
            0x208,
        ] {
            bytes.extend(word.to_le_bytes());
        }
        let data = ReadOnlyData::new(vec![(0x8000, bytes)]);
        let symbols = [
            symbol(0x8000, "vtable for CItem"),
            symbol(0x8048, "vtable for CItemDatabase"),
            symbol(TYPEINFO, "typeinfo for CItem"),
        ];

        let group = vtable_group(&symbols, &data, "CItem").unwrap();
        assert_eq!(
            group.address_points,
            BTreeMap::from([(0, 0x8010), (0x38, 0x8030)])
        );
        assert_eq!(
            group.slots,
            BTreeMap::from([
                (0x100, BTreeSet::from([0])),
                (0x104, BTreeSet::from([0])),
                (0x200, BTreeSet::from([0x38])),
                (0x204, BTreeSet::from([0x38])),
                (0x208, BTreeSet::from([0x38])),
            ])
        );
        assert_eq!(vtable_group(&symbols, &data, "COther"), None);
    }

    /// `adrp` of page `0x5000` then `add` forms `0x5010` directly, through two additions, after
    /// other instructions and a copy, or not at all: an `add` that replaces the page ends it.
    #[test]
    fn functions_forming_an_address_are_found_through_one_or_two_additions() {
        let words: [u32; 24] = [
            // 0x1000: adrp x8, 0x5000; add x8, x8, #0x10
            0x9000_0028,
            0x9100_4108,
            0xd65f_03c0,
            0xd503_201f,
            // 0x1010: adrp x9, 0x5000; add x9, x9, #0x20
            0x9000_0029,
            0x9100_8129,
            0xd65f_03c0,
            0xd503_201f,
            // 0x1020: adrp x10, 0x5000; add x10, x10, #0; add x11, x10, #0x10
            0x9000_002a,
            0x9100_014a,
            0x9100_414b,
            0xd65f_03c0,
            // 0x1030: adrp x8, 0x5000; add x8, x8, #0x158; add x8, x8, #0x10 forms 0x5168
            0x9000_0028,
            0x9105_6108,
            0x9100_4108,
            0xd65f_03c0,
            // 0x1040: adrp x8, 0x5000; four nops; mov x9, x8; add x10, x9, #0x10
            0x9000_0028,
            0xd503_201f,
            0xd503_201f,
            0xd503_201f,
            0xd503_201f,
            0xaa08_03e9,
            0x9100_412a,
            0xd65f_03c0,
        ];
        let code: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        let text = Text {
            address: 0x1000,
            code: &code,
            starts: BTreeSet::from([0x1000, 0x1010, 0x1020, 0x1030, 0x1040]),
        };
        assert_eq!(
            functions_forming(&text, &BTreeSet::from([0x5010])),
            BTreeSet::from([0x1000, 0x1020, 0x1040])
        );

        // Words that do not decode are read for `adrp` and `add` alone.
        let points = BTreeSet::from([0x5010]);
        assert!(words_form(&code[..0x10], 0x1000, &points));
        assert!(!words_form(&code[0x10..0x20], 0x1010, &points));
        assert!(!words_form(&code[0x30..0x40], 0x1030, &points));
    }

    #[test]
    fn pointer_arguments_follow_this_in_x0() {
        assert_eq!(
            pointer_arguments(
                "TSingleObjectGameDatabase<CDb, CItem, false>::ReadExistingEntry(CReader&, CString const&, CItem*, bool)"
            ),
            [1, 2, 3]
        );
        assert_eq!(
            pointer_arguments("CDb::Load(CPdxArray<CString, int> const&, int)"),
            [1]
        );
        assert!(pointer_arguments("CDb::CDb()").is_empty());
    }

    #[test]
    fn thunks_of_a_root_are_named_after_it() {
        let symbols = [
            symbol(0x100, "CItem::InitPostRead()"),
            symbol(0x200, "non-virtual thunk to CItem::InitPostRead()"),
            symbol(
                0x300,
                "{virtual override thunk({offset(-56)}, CItem::InitPostRead())}",
            ),
            symbol(0x400, "non-virtual thunk to COther::InitPostRead()"),
        ];
        let names = Names::new(&symbols);
        assert_eq!(
            thunks(&symbols, &names, &BTreeSet::from([0x100])),
            BTreeMap::from([(0x200, 0x100), (0x300, 0x100)])
        );
    }

    #[test]
    fn a_composer_calls_only_modelled_functions_its_cold_part_or_no_return() {
        use CallKind::*;
        assert!(is_composer(&[]));
        assert!(is_composer(&[OwnColdPart, NeverReturns]));
        assert!(is_composer(&[
            Composing,
            Modelled,
            OwnColdPart,
            NeverReturns
        ]));
        assert!(!is_composer(&[Modelled, NeverReturns]));
        assert!(!is_composer(&[Composing, Other]));

        let symbols = [
            symbol(0x100, "CTag::Build(CPdxStringView)"),
            symbol(0x900, "CTag::Build(CPdxStringView) [clone .cold.1]"),
            symbol(0x980, "COther::Build(CPdxStringView) (.cold.1)"),
            symbol(0xa00, "CString::Reserve(unsigned int)"),
        ];
        let names = Names::new(&symbols);
        let strings = string_functions(&symbols);
        let kind = |target| call_kind(&names, &strings, 0x100, Call::Direct(target));
        assert_eq!(kind(0x900), OwnColdPart);
        assert_eq!(kind(0x980), Other);
        assert_eq!(kind(0xa00), Composing);
        assert_eq!(call_kind(&names, &strings, 0x100, Call::Indirect), Other);
    }

    #[test]
    fn roots_are_named_by_class_and_other_content_classes_are_unnamed() {
        let symbols = [
            symbol(0x10, "CBuildingTypeDatabase::GenerateModifiers()"),
            symbol(
                0x20,
                "TSingleObjectGameDatabase<CBuildingTypeDatabase, CBuildingType, false>::PostReadInit()",
            ),
            symbol(0x30, "CBuildingType::InitPostRead()"),
            symbol(
                0x38,
                "{virtual override thunk({offset(-56)}, CBuildingType::InitPostRead())}",
            ),
            symbol(
                0x3c,
                "{virtual override thunk({offset(-56)}, COtherType::InitPostRead())}",
            ),
            symbol(0x40, "CBuildingType::PostReadInit()"),
            symbol(0x50, "CStrategicResource::InitPostRead()"),
            symbol(0x58, "CStrategicResource::InitPostRead() (.cold.1)"),
            symbol(0x60, "CBuildingTypeDatabase::PostReadInit()"),
        ];
        let registries = [NamedRegistry {
            name: "common/buildings",
            database: "CBuildingTypeDatabase",
            owner: "CBuildingType",
        }];
        let owned = |receiver| RootOf::Registry {
            registry: "common/buildings".into(),
            receiver,
        };

        assert_eq!(
            roots(&symbols, &registries),
            BTreeMap::from([
                (0x10, owned(Receiver::Database)),
                (0x20, owned(Receiver::Database)),
                (0x30, owned(Receiver::Item)),
                (0x50, RootOf::UnnamedContent),
            ])
        );
    }

    #[test]
    fn an_unnamed_input_is_a_parameter_of_its_class() {
        let classes = BTreeSet::from(["CStrategicResource"]);
        assert!(takes_any(
            "CEconomicCategory::FillResourceModifierMatrix(CStrategicResource const&, int&) const",
            &classes
        ));
        assert!(takes_any("f(int, CStrategicResource*)", &classes));
        assert!(!takes_any("f(CStrategicResourceGroup const&)", &classes));
        assert!(!takes_any("CStrategicResource::InitPostRead()", &classes));
    }
}
