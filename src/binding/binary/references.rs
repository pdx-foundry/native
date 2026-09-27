//! Read every reference reader, the functions that its lookup reaches, and each named database's
//! content directory.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::binding::analysis::NamedCandidate;
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::directories::{self, Constructor, Directory};
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::references::{
    ReferenceInput, lambda_operator, reader, shapes::canonical,
};

use super::declarations::Text;

/// How many calls deep the reader reads: the resolver lambda, then the search it calls.
const DEPTH: usize = 2;

/// Demangled names at symbol addresses, at pointer slots that hold a named address, and at
/// import slots, and quoted text at string literals. An address with several different names
/// has none.
pub(in crate::binding) fn names(
    symbols: &[Symbol],
    pointers: &BTreeMap<u64, u64>,
    imports: &BTreeMap<u64, String>,
    strings: &BTreeMap<u64, String>,
) -> BTreeMap<u64, String> {
    let mut by_address: BTreeMap<u64, Option<&str>> = BTreeMap::new();
    for symbol in symbols {
        by_address
            .entry(symbol.address)
            .and_modify(|name| {
                if *name != Some(symbol.name.as_str()) {
                    *name = None;
                }
            })
            .or_insert(Some(symbol.name.as_str()));
    }
    let mut names: BTreeMap<u64, String> = by_address
        .iter()
        .filter_map(|(address, name)| Some((*address, (*name)?.to_owned())))
        .collect();
    for (slot, target) in pointers {
        if let Some(Some(name)) = by_address.get(target) {
            names.insert(*slot, (*name).to_owned());
        }
    }
    names.extend(imports.iter().map(|(slot, name)| (*slot, name.clone())));
    for (address, text) in strings {
        names.entry(*address).or_insert_with(|| format!("{text:?}"));
    }

    names
}

/// The parts of the verified executable that the reference method reads.
pub(in crate::binding) struct Image<'a> {
    pub bytes: &'a [u8],
    pub symbols: &'a [Symbol],
    pub strings: &'a BTreeMap<u64, String>,
    pub pointers: &'a BTreeMap<u64, u64>,
    pub imports: &'a BTreeMap<u64, String>,
}

pub(in crate::binding) fn read(
    image: &Image<'_>,
    candidates: &[NamedCandidate],
) -> Result<ReferenceInput, AnalysisError> {
    let text = Text::read(image.bytes, image.symbols)?;
    let names = names(image.symbols, image.pointers, image.imports, image.strings);
    let readers: BTreeSet<String> = image
        .symbols
        .iter()
        .filter(|symbol| reader(&symbol.name).is_some())
        .map(|symbol| symbol.name.clone())
        .collect();
    let functions = reachable_functions(&text, image.symbols, &names, &readers);
    let directories = database_directories(&text, image, candidates, &readers);

    Ok(ReferenceInput {
        readers,
        functions,
        names,
        directories,
    })
}

/// The decoded bodies of the readers and of the functions that they reach within `DEPTH` calls.
fn reachable_functions(
    text: &Text<'_>,
    symbols: &[Symbol],
    names: &BTreeMap<u64, String>,
    readers: &BTreeSet<String>,
) -> BTreeMap<String, Vec<Instruction>> {
    let addresses = unique_addresses(symbols);
    let mut functions = BTreeMap::new();
    let mut pending: Vec<String> = readers.iter().cloned().collect();
    for _ in 0..=DEPTH {
        let mut reached = Vec::new();
        for name in pending {
            if functions.contains_key(&name) {
                continue;
            }
            let Some(rows) = addresses
                .get(name.as_str())
                .and_then(|address| text.function(*address).ok())
                .and_then(|(address, code)| decode_arm64(code, address).ok())
            else {
                continue;
            };
            reached.extend(reached_functions(&canonical(&rows, names)));
            functions.insert(name, rows);
        }
        pending = reached;
    }

    functions
}

/// The content directory of each database that a reader names: the template join first, then
/// the directory that the database's own loader enumerates.
fn database_directories(
    text: &Text<'_>,
    image: &Image<'_>,
    candidates: &[NamedCandidate],
    readers: &BTreeSet<String>,
) -> BTreeMap<String, Directory> {
    let databases: BTreeSet<&str> = readers
        .iter()
        .filter_map(|callee| Some(reader(callee)?.database))
        .collect();
    let anchors = super::constructors::anchors(image.symbols);

    databases
        .into_iter()
        .map(|database| {
            let directory = template_directory(candidates, database).unwrap_or_else(|| {
                let own = own_functions(text, image.symbols, database);
                directories::loader_directory(&own, &anchors, image.strings)
            });
            (database.to_owned(), directory)
        })
        .collect()
}

/// The functions that a canonical body calls, and the call operators of the resolver lambdas
/// whose vtables it names.
fn reached_functions(lines: &[crate::engine::analysis::references::shapes::Line]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            let value = line.value.as_deref()?;
            if line.text.ends_with("CALL") {
                Some(value.to_owned())
            } else {
                lambda_operator(value)
            }
        })
        .collect()
}

fn template_directory(candidates: &[NamedCandidate], database: &str) -> Option<Directory> {
    candidates
        .iter()
        .find(|candidate| {
            candidate.record.database == database
                && matches!(candidate.directory, Directory::Named(_))
        })
        .map(|candidate| candidate.directory.clone())
}

/// Every function whose name is qualified by `class`.
fn own_functions(text: &Text<'_>, symbols: &[Symbol], class: &str) -> Vec<Constructor> {
    let qualifier = format!("{class}::");

    symbols
        .iter()
        .filter(|symbol| symbol.name.starts_with(&qualifier))
        .filter_map(|symbol| {
            let (address, code) = text.function(symbol.address).ok()?;
            Some(Constructor {
                address,
                code: code.to_vec(),
            })
        })
        .collect()
}

/// The address of each name that has exactly one.
fn unique_addresses(symbols: &[Symbol]) -> BTreeMap<&str, u64> {
    let mut addresses: BTreeMap<&str, BTreeSet<u64>> = BTreeMap::new();
    for symbol in symbols {
        addresses
            .entry(symbol.name.as_str())
            .or_default()
            .insert(symbol.address);
    }

    addresses
        .into_iter()
        .filter(|(_, addresses)| addresses.len() == 1)
        .map(|(name, addresses)| (name, *addresses.first().expect("one address")))
        .collect()
}
