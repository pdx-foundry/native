//! Constructor summaries shared with the nested-object reader method.
use crate::AnalysisError;
use crate::engine::analysis::{declarations::Function, decode::decode_arm64, discovery::Symbol};
use std::collections::{BTreeMap, BTreeSet};

/// Constructor entries reached directly from these object factories, with their compiler
/// vtable-group address points. A class name selects metadata, never grammar behavior.
pub(super) fn constructors(
    bytes: &[u8],
    symbols: &[Symbol],
    pointers: &BTreeMap<u64, u64>,
    bound_slots: &BTreeSet<u64>,
    roots: &[&Function],
) -> Result<BTreeMap<u64, BTreeMap<u64, u64>>, AnalysisError> {
    let mut calls = BTreeSet::new();
    for root in roots {
        let rows =
            decode_arm64(&root.code, root.address).map_err(|_| AnalysisError::InvalidRange)?;
        for row in rows.iter().filter(|row| row.operation == "bl") {
            if let Some(address) = crate::engine::analysis::declarations::number(&row.operands) {
                calls.insert(address);
            }
        }
    }
    let mut classes = BTreeMap::<String, BTreeSet<u64>>::new();
    for symbol in symbols
        .iter()
        .filter(|symbol| calls.contains(&symbol.address))
    {
        let Some((class, method)) = symbol.name.split_once("::") else {
            continue;
        };
        if method.starts_with(&format!("{class}(")) {
            classes
                .entry(class.into())
                .or_default()
                .insert(symbol.address);
        }
    }
    let data = super::language::constant_data(bytes, pointers, bound_slots)?;
    let mut summaries = BTreeMap::new();
    for (class, entries) in classes {
        let Some(group) = super::families::vtable_group(symbols, &data, &class) else {
            continue;
        };
        for entry in entries {
            if summaries
                .insert(entry, group.address_points.clone())
                .is_some()
            {
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
    recipe: &super::super::targets::recipes::PersistentRecipe,
) -> Result<crate::engine::analysis::fields::PersistentInput, AnalysisError> {
    use crate::engine::analysis::fields::{ConcreteReader, PersistentInput};
    let text = super::declarations::Text::read(bytes, symbols)?;
    let entries: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| symbol.name.starts_with(&format!("{owner}::{owner}(")))
        .map(|symbol| symbol.address)
        .collect();
    let mut bodies = Vec::new();
    if entries.len() > 16 {
        return Err(AnalysisError::InvalidRange);
    }
    for entry in entries {
        let (address, code) = text.function(entry)?;
        if code.len() > 65536 {
            return Err(AnalysisError::InvalidRange);
        }
        bodies.push(Function {
            address,
            code: code.to_vec(),
        });
    }
    let roots: Vec<_> = bodies.iter().collect();
    let summaries = constructors(bytes, symbols, pointers, bound_slots, &roots)?;
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
        pointers: pointers.clone(),
        readers,
        never_return: super::families::string_functions(symbols)
            .never_return
            .into_iter()
            .collect(),
    })
}
