//! Constructor summaries shared with the nested-object reader method.
use crate::AnalysisError;
use crate::engine::analysis::{declarations::Function, decode::decode_arm64, discovery::Symbol};
use std::collections::{BTreeMap, BTreeSet};

// Bound decoded constructor input to avoid unbounded analysis of oversized bodies.
const MAX_CONSTRUCTOR_CODE_BYTES: usize = 65536;

fn constructor_class(name: &str) -> Option<&str> {
    let (class, method) = name.split_once("::")?;
    method.starts_with(&format!("{class}(")).then_some(class)
}

/// Constructor entries reached directly or through register-move wrappers, with their compiler
/// vtable-group address points. An absent group gives an empty summary.
/// A class name selects metadata, never grammar behavior.
pub(super) fn constructors(
    bytes: &[u8],
    symbols: &[Symbol],
    pointers: &BTreeMap<u64, u64>,
    bound_slots: &BTreeSet<u64>,
    roots: &[&Function],
) -> Result<BTreeMap<u64, BTreeMap<u64, u64>>, AnalysisError> {
    let text = super::declarations::Text::read(bytes, symbols)?;
    let constructor_entries: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| constructor_class(&symbol.name).is_some())
        .map(|symbol| symbol.address)
        .collect();
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
                let Some(address) = crate::engine::analysis::declarations::number(&row.operands)
                else {
                    continue;
                };
                calls.insert(address);
                if !text.starts.contains(&address) || followed.contains(&address) {
                    continue;
                }
                let (address, code) = text.function(address)?;
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
    for symbol in symbols
        .iter()
        .filter(|symbol| calls.contains(&symbol.address))
    {
        let Some(class) = constructor_class(&symbol.name) else {
            continue;
        };
        classes
            .entry(class.into())
            .or_default()
            .insert(symbol.address);
    }
    let data = super::language::constant_data(bytes, pointers, bound_slots)?;
    let mut summaries = BTreeMap::new();
    for (class, entries) in classes {
        let points = super::families::vtable_group(symbols, &data, &class)
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

/// Owner constructors and the virtual methods installed by their called constructors.
pub(super) fn persistent(
    bytes: &[u8],
    symbols: &[Symbol],
    pointers: &BTreeMap<u64, u64>,
    bound_slots: &BTreeSet<u64>,
    owner: &str,
    recipe: &super::super::targets::PersistentRecipe,
) -> Result<crate::engine::analysis::fields::PersistentInput, AnalysisError> {
    use crate::engine::analysis::fields::{ConcreteReader, PersistentInput};
    let text = super::declarations::Text::read(bytes, symbols)?;
    let entries: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| {
            symbol.name.starts_with(&format!("{owner}::{owner}("))
                && !symbol.name.contains("[clone .cold.")
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
            decode_arm64(&body.code, body.address)
                .is_ok_and(|rows| rows.len() != 1 || rows[0].operation != "b")
        })
        .map(|body| body.address)
        .collect();
    bodies.retain(|body| {
        let Ok(rows) = decode_arm64(&body.code, body.address) else {
            return true;
        };
        // A one-instruction tail branch delegates every input unchanged to a proved full ctor.
        !(rows.len() == 1
            && rows[0].operation == "b"
            && crate::engine::analysis::declarations::number(&rows[0].operands)
                .is_some_and(|target| full_entries.contains(&target)))
    });
    let roots: Vec<_> = bodies.iter().collect();
    let summaries = constructors(bytes, symbols, pointers, bound_slots, &roots)?;
    let mut constructor_bodies = BTreeMap::new();
    for &entry in summaries.keys() {
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
    let name = |address| {
        let names: BTreeSet<_> = symbols
            .iter()
            .filter(|symbol| symbol.address == address)
            .map(|symbol| symbol.name.as_str())
            .collect();
        (names.len() == 1).then(|| names.first().unwrap().to_string())
    };
    let mut pages = BTreeSet::new();
    for body in &bodies {
        for row in
            decode_arm64(&body.code, body.address).map_err(|_| AnalysisError::InvalidRange)?
        {
            if matches!(row.operation.as_str(), "adrp" | "adr")
                && let Some((_, operand)) = row.operands.split_once(',')
                && let Some(address) = crate::engine::analysis::declarations::number(operand)
            {
                pages.insert(address & !0xfff);
            }
        }
    }
    let data = super::language::constant_data(bytes, pointers, bound_slots)?;
    let mut points: BTreeSet<_> = summaries
        .values()
        .flat_map(|points| points.values().copied())
        .collect();
    for symbol in symbols
        .iter()
        .filter(|symbol| pages.contains(&(symbol.address & !0xfff)))
    {
        if let Some(class) = symbol.name.strip_prefix("vtable for ")
            && let Some(group) = super::families::vtable_group(symbols, &data, class)
        {
            points.extend(group.address_points.into_values());
        }
    }
    let mut readers = BTreeMap::new();
    for point in &points {
        let Some(read) = pointers
            .get(&(point + recipe.read_slot))
            .and_then(|&address| name(address))
        else {
            continue;
        };
        let Some(member) = pointers
            .get(&(point + recipe.member_slot))
            .and_then(|&address| name(address))
        else {
            continue;
        };
        if !read.ends_with("::Read(CReader&)") || !member.ends_with("::ReadMember(CReader&, int)") {
            continue;
        }
        let family = recipe
            .families
            .iter()
            .find_map(|(anchor, family)| (*anchor == read).then_some(*family))
            .unwrap_or(crate::BlockFamily::Unknown);
        readers.insert(
            *point,
            ConcreteReader {
                read,
                member,
                family,
            },
        );
    }
    let constructors = bodies
        .into_iter()
        .map(|body| crate::engine::analysis::fields::Function {
            address: body.address,
            code: body.code,
            name: name(body.address).unwrap_or_default(),
        })
        .collect();
    Ok(PersistentInput {
        constructors,
        summaries,
        constructor_bodies,
        pointers: pointers.clone(),
        readers,
        never_return: super::families::string_functions(symbols)
            .never_return
            .into_iter()
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::{analysis_support::macho_with_text, assembler::arm64};

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
        let recipe = super::super::super::targets::PersistentRecipe {
            string_array: [0; 3],
            compound_sizes: [0; 3],
            value_token: 0,
            token_text: 0,
            read_slot: 0,
            member_slot: 0,
            families: &[],
        };
        let bind = |symbols: &[Symbol]| {
            persistent(
                &bytes,
                symbols,
                &BTreeMap::new(),
                &BTreeSet::new(),
                "CExample",
                &recipe,
            )
            .unwrap()
        };
        let input = bind(&symbols);
        assert_eq!(input.constructors.len(), 1);
        assert_eq!(input.constructors[0].address, 0x1000);

        symbols[2].name = "CExample::CExample(int)".into();
        assert_eq!(bind(&symbols).constructors.len(), 2);
    }
}
