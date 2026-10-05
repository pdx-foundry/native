//! Read the modifier node graph, the node constructor calls and the static initializers.
use std::collections::{BTreeMap, BTreeSet};

use object::{Object, ObjectSection, ObjectSegment};

use crate::AnalysisError;
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::modifier_nodes::{self, Construction, ModifierNodeInput};
use crate::engine::analysis::stop::Unresolved;

use super::super::targets::DeclarationRecipe;
use super::declarations::Text;
use super::language::{categories, constant_data};

/// A node type, up to its node number.
const NODE_TYPE: &str =
    "NModifierNode::CModifierNode<CModifier, EModifierNodeCategory, (EModifierNodeCategory)";
/// What follows a node type's number, up to its first source node.
const SOURCES: &str = ", NModifierNode::SDependencies<EModifierNodeCategory, ";
/// A node base constructor, up to the node type that it builds.
const BASE_CONSTRUCTOR: &str =
    "NModifierNode::CModifierNodeBase<CModifier, EModifierNodeCategory>::CModifierNodeBase<";
/// What follows the node type in a base constructor, up to the owner type.
const OWNER: &str = "::CModifierNode<";

/// Read every node type symbol, every direct call to a node base constructor, and the static
/// initializers.
pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    pointers: &BTreeMap<u64, u64>,
    bound_slots: &BTreeSet<u64>,
    recipe: &DeclarationRecipe,
) -> Result<ModifierNodeInput, AnalysisError> {
    let text = Text::read(bytes, symbols)?;
    let mut sources = BTreeMap::new();

    for symbol in symbols {
        for (at, _) in symbol.name.match_indices(NODE_TYPE) {
            node_type(&symbol.name[at..], &mut sources);
        }
    }

    let mut constructions = Vec::new();
    for symbol in symbols {
        let Some((node, owner)) = base_constructor(&symbol.name) else {
            continue;
        };

        for site in text.direct_calls(symbol.address) {
            let Some(&caller) = text.starts.range(..=site).next_back() else {
                continue;
            };
            constructions.push(Construction {
                node,
                owner: owner.clone(),
                caller,
                site,
            });
        }
    }
    if constructions.is_empty() {
        return Err(AnalysisError::InvalidRange);
    }

    Ok(ModifierNodeInput {
        sources,
        constructions,
        initializers: initializers(bytes)?,
        text: modifier_nodes::Text {
            address: text.address,
            bytes: text.code.to_vec(),
            starts: text.starts.clone(),
        },
        data: constant_data(bytes, pointers, bound_slots)?,
        layout: recipe.modifier_nodes,
        categories: categories(bytes, symbols, &text, recipe)?,
    })
}

/// Parse the node type at the start of `text`, record the sources of it and of each node nested
/// in it, and return its node and the text after it.
fn node_type<'a>(
    text: &'a str,
    sources: &mut BTreeMap<u32, Result<Vec<u32>, Unresolved>>,
) -> Option<(u32, &'a str)> {
    let rest = text.strip_prefix(NODE_TYPE)?;
    let digits = rest.find(|character: char| !character.is_ascii_digit())?;
    let node = rest[..digits].parse().ok()?;
    let mut rest = rest[digits..].strip_prefix(SOURCES)?;
    let mut nested = Vec::new();

    while rest.starts_with(NODE_TYPE) {
        let (source, after) = node_type(rest, sources)?;
        nested.push(source);
        rest = after.strip_prefix(", ").unwrap_or(after);
    }

    let rest = rest.trim_start().strip_prefix('>')?;
    let rest = rest.trim_start().strip_prefix('>')?;
    record(sources, node, nested);
    Some((node, rest))
}

fn record(sources: &mut BTreeMap<u32, Result<Vec<u32>, Unresolved>>, node: u32, nested: Vec<u32>) {
    let known = sources.entry(node).or_insert_with(|| Ok(nested.clone()));
    if known.as_ref().is_ok_and(|known| *known != nested) {
        *known = Err(Unresolved::new("node-sources"));
    }
}

/// The node and owner type of a node base constructor. Its template argument is a closure type
/// inside the node's constructor template, which can itself be nested in that template, so the
/// owner follows the last repetition of the node type.
fn base_constructor(name: &str) -> Option<(u32, String)> {
    let mut rest = name.strip_prefix(BASE_CONSTRUCTOR)?;
    let mut node = None;

    while rest.starts_with(NODE_TYPE) {
        let (nested, after) = node_type(rest, &mut BTreeMap::new())?;
        if node.is_some_and(|node| node != nested) {
            return None;
        }

        node = Some(nested);
        rest = after.strip_prefix(OWNER)?;
    }

    let owner = &rest[..rest.find(',')?];
    Some((node?, owner.into()))
}

/// Entry of each static initializer, from `__init_offsets`: offsets from the image's header.
fn initializers(bytes: &[u8]) -> Result<Vec<u64>, AnalysisError> {
    let slice = super::selected_slice(bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let file = object::File::parse(slice).map_err(|_| AnalysisError::InvalidRange)?;
    let header = file
        .segments()
        .find(|segment| segment.name().ok().flatten() == Some("__TEXT"))
        .ok_or(AnalysisError::InvalidRange)?
        .address();
    let offsets = file
        .section_by_name("__init_offsets")
        .ok_or(AnalysisError::InvalidRange)?
        .data()
        .map_err(|_| AnalysisError::InvalidRange)?;

    Ok(offsets
        .as_chunks::<4>()
        .0
        .iter()
        .map(|offset| header + u64::from(u32::from_le_bytes(*offset)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(number: u32, sources: &[String]) -> String {
        let sources: String = sources.iter().map(|source| format!("{source}, ")).collect();
        let sources = sources.strip_suffix(", ").unwrap_or(&sources);
        format!("{NODE_TYPE}{number}{SOURCES}{sources} > >")
    }

    #[test]
    fn nested_node_types_give_each_node_its_sources() {
        let leaf = node(4, &[]);
        let middle = node(17, &[leaf.clone(), node(13, &[])]);
        let mut sources = BTreeMap::new();

        let symbol = format!("{}&) const", node(18, &[middle, leaf]));
        let (top, rest) = node_type(&symbol, &mut sources).unwrap();

        assert_eq!(top, 18);
        assert_eq!(rest, "&) const");
        assert_eq!(sources[&18], Ok(vec![17, 4]));
        assert_eq!(sources[&17], Ok(vec![4, 13]));
        assert_eq!(sources[&4], Ok(vec![]));
    }

    #[test]
    fn two_symbols_that_disagree_on_sources_leave_the_node_unresolved() {
        let mut sources = BTreeMap::new();

        node_type(&node(2, &[node(10, &[])]), &mut sources).unwrap();
        node_type(&node(2, &[node(26, &[])]), &mut sources).unwrap();

        assert_eq!(sources[&2].as_ref().unwrap_err().reason, "node-sources");
        assert_eq!(sources[&10], Ok(vec![]));
    }

    #[test]
    fn a_base_constructor_names_its_node_and_owner() {
        let ship = node(32, &[node(24, &[])]);
        let name = format!(
            "{BASE_CONSTRUCTOR}{ship}{OWNER}{ship}{OWNER}CShip, int, ModifierCategory>(void (CShip::*)() const)"
        );

        assert_eq!(base_constructor(&name), Some((32, "CShip".into())));
    }
}
