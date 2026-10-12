//! Read the input of the loader owner join (`engine::analysis::discovery::loader_owner`) for the
//! databases that their own loader names.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::discovery::loader_owner::{LoaderOwner, loader_owner};

use super::declarations::Text;
use super::references::Image;

/// The member reader whose class can own a registry's items.
const MEMBER_READER: &str = "::ReadMember(CReader&, int)";

/// The owner that the loader of each of `databases` builds, keyed by database type.
pub(in crate::binding) fn loader_owners(
    text: &Text<'_>,
    image: &Image<'_>,
    databases: &BTreeSet<String>,
) -> Result<BTreeMap<String, LoaderOwner>, AnalysisError> {
    let enumerations = super::constructors::anchors(image.symbols).file_enumerations;
    let builders = builders(image)?;

    Ok(databases
        .iter()
        .map(|database| {
            let functions = own_functions(text, image.symbols, database);
            let owner = loader_owner(&functions, &enumerations, &builders);

            (database.clone(), owner)
        })
        .collect())
}

/// The class that each constructor body and each primary vtable address point builds, for every
/// class with a member reader.
fn builders(image: &Image<'_>) -> Result<BTreeMap<u64, String>, AnalysisError> {
    let readers: BTreeSet<&str> = image
        .symbols
        .iter()
        .filter_map(|symbol| symbol.name.strip_suffix(MEMBER_READER))
        .collect();
    let mut builders: BTreeMap<u64, String> = image
        .symbols
        .iter()
        .filter_map(|symbol| {
            let (class, false) = super::type_pointers::constructed_class(&symbol.name)? else {
                return None;
            };

            readers
                .contains(class)
                .then(|| (symbol.address, class.to_owned()))
        })
        .collect();

    let data = super::language::constant_data(image.bytes, image.pointers, image.bound_slots)?;
    let indexed = image.index.over(image.symbols);
    for class in readers {
        if let Some(point) = indexed
            .vtable_group(&data, class)
            .and_then(|group| group.address_points.get(&0).copied())
        {
            builders.insert(point, class.to_owned());
        }
    }

    Ok(builders)
}

/// The decoded body of every function whose name is qualified by `class`, by its start.
fn own_functions(
    text: &Text<'_>,
    symbols: &[Symbol],
    class: &str,
) -> BTreeMap<u64, Vec<Instruction>> {
    let qualifier = format!("{class}::");

    symbols
        .iter()
        .filter(|symbol| symbol.name.starts_with(&qualifier))
        .filter_map(|symbol| {
            let (address, code) = text.function(symbol.address).ok()?;

            Some((address, decode_arm64(code, address).ok()?))
        })
        .collect()
}
