//! Read the input of the loader owner join (`engine::analysis::discovery::loader_owner`) for the
//! databases that their own loader names.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::discovery::loader_owner::{LoaderOwner, LoaderReads, loader_owner};

use super::declarations::Text;
use super::references::Image;

/// The member reader whose class can own a registry's items.
const MEMBER_READER: &str = "::ReadMember(CReader&, int)";

/// The owner that the loader of each of `databases` builds, keyed by database type.
/// `token_offset` is the reader offset of a statement's token word.
pub(in crate::binding) fn loader_owners(
    text: &Text<'_>,
    image: &Image<'_>,
    databases: &BTreeSet<String>,
    token_offset: u64,
) -> Result<BTreeMap<String, LoaderOwner>, AnalysisError> {
    let enumerations = super::constructors::anchors(image.symbols).file_enumerations;
    let statement_reads = image
        .index
        .over(image.symbols)
        .named(super::fields::STATEMENT_READER)
        .map(|symbol| symbol.address)
        .collect();
    let token_names = text.token_names(image.symbols, image.strings)?;
    let reads = LoaderReads {
        enumerations: &enumerations,
        statement_reads: &statement_reads,
        token_offset,
        token_names: &token_names,
    };
    let builders = builders(image)?;

    Ok(databases
        .iter()
        .map(|database| {
            let functions = own_functions(text, image.symbols, database);
            let owner = loader_owner(&functions, &reads, &builders);

            (database.clone(), owner)
        })
        .collect())
}

/// Why the loader of `database` builds no class with a member reader, as read on M452. A database
/// with no stated reason gets the general one. Each stated reason holds while its loader keeps
/// that shape; the knowledge page (`registry-fields.md`, "Custom-loader owners") gives the
/// functions and what would answer the registry.
pub(in crate::binding) fn no_owner_reason(database: &str) -> &'static str {
    match database {
        "CColorsDatabase" => {
            "the loader reads fixed root keys of unnamed colours into its database and builds no item"
        }
        "CWeaponTagDatabase" => {
            "the loader registers each token of its files as a tag and builds no item"
        }
        "CShipBehaviorDatabase" => {
            "the loader reads each item in its own statement loop, not through a member reader"
        }
        "CWorldGfxDatabase" => {
            "the loader reads the database itself, whose member reader builds and reads each item"
        }
        _ => "the loader builds no class with a member reader",
    }
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
