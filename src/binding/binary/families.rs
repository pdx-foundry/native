//! Read the inputs of the modifier-family method from executable text.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::engine::analysis::{
    decode::decode_arm64,
    discovery::Symbol,
    evaluate::Code,
    families::{
        DatabaseLayout, Definitions, FamilyInput, KeyStorageInput, Receiver, RegistryInput, Root,
        StringFunctions, StringLayout,
        joins::{self, DEPTH, GenerationSite, Graph, Joins, RootOf},
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
            data: constant_data(bytes, facts.pointers, facts.bound_slots)?,
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
    let unnamed_inputs = callers
        .iter()
        .flat_map(|(function, callers)| callers.iter().chain([function]))
        .copied()
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
    let own_cold = names.of(target).any(|cold| {
        names.of(function).any(|name| {
            cold.strip_prefix(name)
                .is_some_and(|suffix| suffix.contains(".cold."))
        })
    });
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
