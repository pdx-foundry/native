//! Constructor summaries shared with the nested-object reader method.
use super::{declarations::Text, families::Names, references::Image, symbols::IndexedSymbols};
use crate::AnalysisError;
use crate::binding::targets::{AnchorSlot, PersistentRecipe};
use crate::engine::analysis::{
    declarations::{Function, branch_alias},
    decode::decode_arm64,
    discovery::Symbol,
    evaluate::ReadOnlyData,
    fields::ModifierContainers,
};
use std::collections::{BTreeMap, BTreeSet};

// Bound decoded constructor input to avoid unbounded analysis of oversized bodies.
const MAX_CONSTRUCTOR_CODE_BYTES: usize = 65536;

/// The class of a constructor symbol, `Class::Class(…)`, `Outer::Class::Class(…)` or
/// `Class<Arguments>::Class(…)`. The class is the name before the last scope separator outside
/// template arguments, so a nested class's constructor names the nested class.
pub(super) fn constructor_class(name: &str) -> Option<&str> {
    let separator = last_top_level_scope(name)?;
    let (class, method) = (&name[..separator], &name[separator + 2..]);
    let last = last_top_level_scope(class).map_or(class, |at| &class[at + 2..]);
    let base = last.split_once('<').map_or(last, |(base, _)| base);

    method.strip_prefix(base)?.starts_with('(').then_some(class)
}

/// The byte offset of the last `::` outside template arguments, before any parameter list.
fn last_top_level_scope(name: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut last = None;

    for (at, character) in name.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.checked_sub(1)?,
            '(' if depth == 0 => break,
            ':' if depth == 0 && name[at..].starts_with("::") => last = Some(at),
            _ => {}
        }
    }

    last
}

/// Constructor entries reached directly or through register-move wrappers, with their compiler
/// vtable-group address points. An absent group gives an empty summary.
/// A class name selects metadata, never grammar behavior.
pub(super) fn constructors(
    text: &Text<'_>,
    symbols: IndexedSymbols<'_>,
    data: &ReadOnlyData,
    roots: &[&Function],
) -> Result<BTreeMap<u64, BTreeMap<u64, u64>>, AnalysisError> {
    let constructor_entries = symbols.constructors();
    let mut calls = BTreeSet::new();
    let mut pending: Vec<_> = roots.iter().map(|body| (*body).clone()).collect();
    let mut followed = BTreeSet::new();
    // Only constructor code and complete register-move wrappers extend this bounded graph.
    for _ in 0..8 {
        let mut next = Vec::new();
        for body in pending {
            let rows =
                decode_arm64(&body.code, body.address).map_err(|_| AnalysisError::InvalidRange)?;
            for row in rows
                .iter()
                .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
            {
                let Some(address) = crate::engine::analysis::decode::number(&row.operands) else {
                    continue;
                };
                calls.insert(address);
                if followed.contains(&address) {
                    continue;
                }
                let Ok((address, code)) = text.function(address) else {
                    continue;
                };
                let callee = Function {
                    address,
                    code: code.to_vec(),
                };
                let wrapper =
                    crate::engine::analysis::declarations::register_move_tail_target(&callee)
                        .is_some_and(|target| constructor_entries.contains(&target));
                if constructor_entries.contains(&address) || wrapper {
                    followed.insert(address);
                    next.push(callee);
                }
            }
        }
        pending = next;
        if pending.is_empty() {
            break;
        }
    }
    let mut classes = BTreeMap::<String, BTreeSet<u64>>::new();
    for symbol in symbols.at(&calls) {
        let Some(class) = constructor_class(&symbol.name) else {
            continue;
        };
        classes
            .entry(class.into())
            .or_default()
            .insert(symbol.address);
    }
    let mut summaries = BTreeMap::new();
    for (class, entries) in classes {
        let points = symbols
            .vtable_group(data, &class)
            .map(|group| group.address_points)
            .unwrap_or_default();
        for entry in entries {
            if summaries.insert(entry, points.clone()).is_some() {
                return Err(AnalysisError::InvalidRange);
            }
        }
    }
    Ok(summaries)
}

/// Import effects, allocators and small engine accessors used by the constructor evaluator.
/// Accessor names select code to execute; their bodies still determine all reads and writes.
/// Allocator names select the allocation rule in place of their bodies.
pub(super) fn constructor_calls(
    text: &Text<'_>,
    symbols: &[Symbol],
) -> Result<crate::engine::analysis::ConstructorCalls, AnalysisError> {
    use crate::engine::analysis::ConstructorCalls;
    let mut calls = ConstructorCalls::default();
    for symbol in symbols {
        match symbol.name.as_str() {
            "_strlen" => {
                calls.lengths.insert(symbol.address);
            }
            "_memcpy" => {
                calls.copies.insert(symbol.address);
            }
            "_memmove" => {
                calls.moves.insert(symbol.address);
            }
            "operator new(unsigned long)"
            | "operator new[](unsigned long)"
            | "CPdxCommonStringAllocator::allocate(unsigned long, void const*)" => {
                calls.allocators.insert(symbol.address);
            }
            "CString::GetTCharPtr() const" | "CString::GetSize() const" => {
                let (at, body) = text.function(symbol.address)?;
                if body.len() <= MAX_CONSTRUCTOR_CODE_BYTES {
                    calls.helpers.insert(at, body.to_vec());
                }
            }
            _ => {}
        }
    }
    Ok(calls)
}

/// The persistent reader at one vtable address point, with the family of the recipe anchor that
/// its read or member slot holds. `name` gives the unique symbol at an address.
pub(super) fn concrete_reader(
    recipe: &PersistentRecipe,
    pointers: &BTreeMap<u64, u64>,
    point: u64,
    name: &dyn Fn(u64) -> Option<String>,
) -> Option<crate::engine::analysis::fields::ConcreteReader> {
    let slot = |offset: u64| {
        pointers
            .get(&point.checked_add(offset)?)
            .and_then(|&at| name(at))
    };
    let read = slot(recipe.read_slot).filter(|read| read.ends_with("::Read(CReader&)"))?;
    let member = slot(recipe.member_slot)
        .filter(|member| member.ends_with("::ReadMember(CReader&, int)"))?;
    let anchor = recipe.families.iter().find(|anchor| {
        let held = match anchor.slot {
            AnchorSlot::Read => &read,
            AnchorSlot::Member => &member,
        };

        anchor.symbol == held
    });

    Some(crate::engine::analysis::fields::ConcreteReader {
        family: anchor.map_or(crate::BlockFamily::Unknown, |anchor| anchor.family),
        delegate: anchor
            .and_then(|anchor| anchor.delegate_slot)
            .and_then(slot),
        post_read: recipe.post_read_slots.map(slot),
        read,
        member,
    })
}

/// The parts of a persistent input that every owner shares.
pub(super) struct SharedPersistent {
    constructor_calls: crate::engine::analysis::ConstructorCalls,
    writable_slots: BTreeSet<u64>,
    never_return: Vec<u64>,
    containers: ModifierContainers,
}

impl SharedPersistent {
    /// Read them from `image`, whose text is `text`.
    pub(super) fn read(
        image: &Image<'_>,
        text: &Text<'_>,
        recipe: &PersistentRecipe,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            constructor_calls: constructor_calls(text, image.symbols)?,
            writable_slots: super::language::writable_slots(image.bytes, image.pointers)?,
            never_return: super::families::never_return(image.symbols)
                .into_iter()
                .collect(),
            containers: modifier_containers(image.symbols, recipe),
        })
    }
}

/// Owner constructors and the virtual methods installed by their called constructors. `text` is
/// the image's text, `data` its constant data and `shared` its shared persistent input, which
/// every call shares.
pub(super) fn persistent(
    image: &Image<'_>,
    text: &Text<'_>,
    data: &ReadOnlyData,
    shared: &SharedPersistent,
    owner: &str,
    recipe: &PersistentRecipe,
) -> Result<crate::engine::analysis::fields::PersistentInput, AnalysisError> {
    let symbols = image.index.over(image.symbols);
    let names = Names::new(symbols.symbols);
    let entries: BTreeSet<_> = symbols
        .constructors_of(owner)
        .into_iter()
        .filter(|symbol| {
            !symbol.name.contains("[clone .cold.")
                // A branch-island stub outside the text section only jumps to a selected body.
                && text.starts.contains(&symbol.address)
        })
        .map(|symbol| symbol.address)
        .collect();
    let mut bodies = Vec::new();
    if entries.len() > 16 {
        return Err(AnalysisError::InvalidRange);
    }
    for entry in entries {
        let (address, code) = text.function(entry)?;
        if code.len() > MAX_CONSTRUCTOR_CODE_BYTES {
            return Err(AnalysisError::InvalidRange);
        }
        bodies.push(Function {
            address,
            code: code.to_vec(),
        });
    }
    let full_entries: BTreeSet<_> = bodies
        .iter()
        .filter(|body| {
            decode_arm64(&body.code, body.address).is_ok_and(|rows| branch_alias(&rows).is_none())
        })
        .map(|body| body.address)
        .collect();
    let full_names: BTreeSet<_> = full_entries
        .iter()
        .flat_map(|&entry| names.of(entry))
        .collect();
    // A branch-island stub outside the text section carries the name of the body it reaches.
    let full_stub = |target: u64| {
        !text.starts.contains(&target) && names.of(target).any(|name| full_names.contains(name))
    };
    bodies.retain(|body| {
        let Ok(rows) = decode_arm64(&body.code, body.address) else {
            return true;
        };
        // An alias delegates every input unchanged to a proved full constructor.
        !branch_alias(&rows)
            .is_some_and(|target| full_entries.contains(&target) || full_stub(target))
    });

    persistent_from(image, text, data, shared, recipe, bodies)
}

/// The persistent input whose constructor bodies are `bodies`: each runs with the object in `x0`
/// and returns once the object is built.
pub(super) fn persistent_from(
    image: &Image<'_>,
    text: &Text<'_>,
    data: &ReadOnlyData,
    shared: &SharedPersistent,
    recipe: &PersistentRecipe,
    bodies: Vec<Function>,
) -> Result<crate::engine::analysis::fields::PersistentInput, AnalysisError> {
    use crate::engine::analysis::fields::PersistentInput;
    let symbols = image.index.over(image.symbols);
    let names = Names::new(symbols.symbols);
    let roots: Vec<_> = bodies.iter().collect();
    let summaries = constructors(text, symbols, data, &roots)?;
    let mut constructor_bodies = BTreeMap::new();
    // A branch-island stub has no body to enter; its summary still installs its vtables.
    for &entry in summaries.keys().filter(|entry| text.starts.contains(entry)) {
        let (address, code) = text.function(entry)?;
        if code.len() <= MAX_CONSTRUCTOR_CODE_BYTES {
            constructor_bodies.insert(
                address,
                crate::engine::analysis::fields::Function {
                    address,
                    code: code.to_vec(),
                    name: String::new(),
                },
            );
        }
    }
    let name = |address| names.unique(address).map(str::to_owned);
    let mut pages = BTreeSet::new();
    let entered = constructor_bodies
        .values()
        .map(|body| (body.address, &body.code));
    for (address, code) in bodies
        .iter()
        .map(|body| (body.address, &body.code))
        .chain(entered)
    {
        for row in decode_arm64(code, address).map_err(|_| AnalysisError::InvalidRange)? {
            if matches!(row.operation.as_str(), "adrp" | "adr")
                && let Some((_, operand)) = row.operands.split_once(',')
                && let Some(address) = crate::engine::analysis::decode::number(operand)
            {
                pages.insert(address & !0xfff);
            }
        }
    }
    let mut points: BTreeSet<_> = summaries
        .values()
        .flat_map(|points| points.values().copied())
        .collect();
    for symbol in pages
        .iter()
        .flat_map(|&page| symbols.between(page..page.saturating_add(0x1000)))
    {
        if let Some(class) = symbol.name.strip_prefix("vtable for ")
            && let Some(group) = symbols.vtable_group(data, class)
        {
            points.extend(group.address_points.into_values());
        }
    }
    let readers = points
        .iter()
        .filter_map(|&point| {
            Some((
                point,
                concrete_reader(recipe, image.pointers, point, &name)?,
            ))
        })
        .collect();
    let constructors = bodies
        .into_iter()
        .map(|body| crate::engine::analysis::fields::Function {
            address: body.address,
            code: body.code,
            name: name(body.address).unwrap_or_default(),
        })
        .collect();
    Ok(PersistentInput {
        constructor_calls: shared.constructor_calls.clone(),
        constructors,
        summaries,
        constructor_bodies,
        pointers: image.pointers.clone(),
        writable_slots: shared.writable_slots.clone(),
        readers,
        never_return: shared.never_return.clone(),
        requested_words: Default::default(),
        containers: shared.containers.clone(),
        initialized_words: Default::default(),
    })
}

/// The default constructors of a modifier container. Each stores the mask of every category.
const DEFAULT_CONTAINERS: [&str; 3] = [
    "CStaticModifier::CStaticModifier()",
    "CStaticModifier::CStaticModifier(CString const&)",
    "CCustomDescriptionModifier::CCustomDescriptionModifier()",
];

/// The constructors that store a modifier container's category mask: each constructor whose first
/// arguments are an int and the mask, and the default constructors. A branch-island stub carries
/// the name of its body, so it is one of them.
fn modifier_containers(symbols: &[Symbol], recipe: &PersistentRecipe) -> ModifierContainers {
    let mut containers = ModifierContainers {
        mask_offset: recipe.container_mask_offset,
        ..Default::default()
    };

    for symbol in symbols {
        if DEFAULT_CONTAINERS.contains(&symbol.name.as_str()) {
            containers.default.insert(symbol.address);
        } else if constructor_class(&symbol.name).is_some()
            && symbol.name.contains("(int, ModifierCategory")
        {
            containers.category.insert(symbol.address);
        }
    }

    containers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::{analysis_support::macho_with_text, assembler::arm64};

    /// The persistent input of `owner` in an image that has no pointers, strings or imports.
    fn bind_constructors(
        bytes: &[u8],
        symbols: &[Symbol],
        owner: &str,
        recipe: &PersistentRecipe,
    ) -> crate::engine::analysis::fields::PersistentInput {
        let (strings, pointers, imports, bound_slots) = Default::default();
        let image = Image {
            bytes,
            symbols,
            strings: &strings,
            pointers: &pointers,
            imports: &imports,
            bound_slots: &bound_slots,
            index: &super::super::symbols::SymbolIndex::new(symbols),
        };
        let text = Text::read(bytes, symbols).unwrap();
        let data = ReadOnlyData::default();
        let shared = SharedPersistent::read(&image, &text, recipe).unwrap();

        persistent(&image, &text, &data, &shared, owner, recipe).unwrap()
    }

    #[test]
    fn a_constructor_names_its_class_with_template_arguments() {
        assert_eq!(constructor_class("A::A(int)"), Some("A"));
        assert_eq!(constructor_class("A<B<C>>::A(int)"), Some("A<B<C>>"));
        assert_eq!(constructor_class("A<x::Y>::A()"), Some("A<x::Y>"));
        assert_eq!(constructor_class("A::B::B()"), Some("A::B"));
        assert_eq!(constructor_class("A<x::Y>::B::B()"), Some("A<x::Y>::B"));
        assert_eq!(constructor_class("A::B::A()"), None);
        assert_eq!(constructor_class("A<B>::~A()"), None);
        assert_eq!(constructor_class("A::Read(B::C)"), None);
    }

    #[test]
    fn an_alias_to_a_stub_of_a_full_constructor_is_not_a_second_constructor() {
        let code = arm64!(at 0x1000;
            ret;
            b extern 0x9000
        );
        let bytes = macho_with_text(&code);
        let constructor = "CExample<A>::CExample(int)";
        let mut symbols = vec![
            Symbol {
                name: constructor.into(),
                address: 0x1000,
            },
            Symbol {
                name: constructor.into(),
                address: 0x1004,
            },
            Symbol {
                name: constructor.into(),
                address: 0x9000,
            },
        ];
        let recipe = PersistentRecipe {
            string_array: [0; 3],
            compound_sizes: [0; 3],
            value_token: 0,
            token_text: 0,
            read_slot: 0,
            member_slot: 0,
            post_read_slots: [0; 2],
            container_mask_offset: 0xac,
            families: &[],
        };
        let bind = |symbols: &[Symbol]| bind_constructors(&bytes, symbols, "CExample<A>", &recipe);

        assert_eq!(bind(&symbols).constructors.len(), 1);

        symbols[2].name = "COther::COther(int)".into();
        assert_eq!(bind(&symbols).constructors.len(), 2);
    }

    #[test]
    fn owner_constructors_exclude_outlined_cleanup_fragments_and_keep_complete_aliases() {
        let code = arm64!(at 0x1000;
            ret;
            b extern 0x1000;
            ret
        );
        let bytes = macho_with_text(&code);
        let mut symbols = vec![
            Symbol {
                name: "CExample::CExample()".into(),
                address: 0x1000,
            },
            Symbol {
                name: "CExample::CExample()".into(),
                address: 0x1004,
            },
            Symbol {
                name: "CExample::CExample() [clone .cold.1]".into(),
                address: 0x1008,
            },
        ];
        let recipe = PersistentRecipe {
            string_array: [0; 3],
            compound_sizes: [0; 3],
            value_token: 0,
            token_text: 0,
            read_slot: 0,
            member_slot: 0,
            post_read_slots: [0; 2],
            container_mask_offset: 0xac,
            families: &[],
        };
        let bind = |symbols: &[Symbol]| bind_constructors(&bytes, symbols, "CExample", &recipe);
        let input = bind(&symbols);
        assert_eq!(input.constructors.len(), 1);
        assert_eq!(input.constructors[0].address, 0x1000);

        symbols[2].name = "CExample::CExample(int)".into();
        assert_eq!(bind(&symbols).constructors.len(), 2);
    }

    #[test]
    fn a_nested_class_constructor_called_by_the_owner_is_entered() {
        let code = arm64!(at 0x1000;
            bl extern 0x1008;
            ret;
            ret
        );
        let bytes = macho_with_text(&code);
        let symbols = vec![
            Symbol {
                name: "CExample::CExample()".into(),
                address: 0x1000,
            },
            Symbol {
                name: "CExample::SData::SData()".into(),
                address: 0x1008,
            },
        ];
        let recipe = PersistentRecipe {
            string_array: [0; 3],
            compound_sizes: [0; 3],
            value_token: 0,
            token_text: 0,
            read_slot: 0,
            member_slot: 0,
            post_read_slots: [0; 2],
            container_mask_offset: 0xac,
            families: &[],
        };
        let input = bind_constructors(&bytes, &symbols, "CExample", &recipe);

        assert!(input.summaries.contains_key(&0x1008));
        assert!(input.constructor_bodies.contains_key(&0x1008));
    }
}
