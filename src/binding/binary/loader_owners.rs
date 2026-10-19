//! Read the input of the loader owner join (`engine::analysis::discovery::loader_owner`) for the
//! databases that their own loader names.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::directories::{self, Directory};
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::discovery::loader_owner::{
    LoaderOwner, LoaderReads, directory_owner, loader_owner,
};

use super::declarations::Text;
use super::references::Image;

/// The member reader whose class can own a registry's items.
const MEMBER_READER: &str = "::ReadMember(CReader&, int)";

/// The owner that the loader of each database in `directories` builds for each directory that
/// it names, keyed by database type and directory. A database that names several directories
/// builds the items of each from that directory's own enumeration calls. `token_offset` is the
/// reader offset of a statement's token word.
pub(in crate::binding) fn loader_owners(
    text: &Text<'_>,
    image: &Image<'_>,
    directories: &BTreeMap<String, Directory>,
    token_offset: u64,
) -> Result<BTreeMap<(String, String), LoaderOwner>, AnalysisError> {
    let anchors = super::constructors::anchors(image.symbols);
    let statement_reads = image
        .index
        .over(image.symbols)
        .named(super::fields::STATEMENT_READER)
        .map(|symbol| symbol.address)
        .collect();
    let token_names = text.token_names(image.symbols, image.strings)?;
    let reads = LoaderReads {
        enumerations: &anchors.file_enumerations,
        statement_reads: &statement_reads,
        token_offset,
        token_names: &token_names,
    };
    let builders = builders(image)?;
    let mut owners = BTreeMap::new();

    for (database, directory) in directories {
        let functions = own_functions(text, image.symbols, database);
        match directory {
            Directory::Named(name) => {
                let owner = loader_owner(&functions, &reads, &builders);
                owners.insert((database.clone(), name.clone()), owner);
            }
            Directory::Ambiguous(names) => {
                let sites = directories::enumerated_directories(
                    &super::references::own_functions(text, image.symbols, database),
                    &super::references::instance_members(text, image, database, &anchors),
                    &anchors,
                    image.strings,
                    image.pointers,
                );
                for name in names {
                    let calls: Vec<u64> = sites
                        .iter()
                        .filter(|(_, literal)| literal.as_ref() == Some(name))
                        .map(|(call, _)| *call)
                        .collect();
                    let owner = directory_owner(&functions, &reads, &builders, &calls);
                    owners.insert((database.clone(), name.clone()), owner);
                }
            }
            Directory::Missing => {}
        }
    }

    Ok(owners)
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
