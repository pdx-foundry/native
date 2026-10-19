//! Read every reference reader, the trigger lookup and every owner initializer, the functions that
//! their lookups reach, every validity check, and each named database's content directory.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::binding::analysis::NamedCandidate;
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::directories::{self, Constructor, Directory};
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::loader_rules;
use crate::engine::analysis::readers::TRIGGER_LOOKUP;
use crate::engine::analysis::references::{
    ReferenceInput, initialization, lambda_operator, reader,
    shapes::{Line, canonical},
};

use super::declarations::Text;

/// How many calls deep the reader reads: the resolver lambda or the initializer's getter, then the
/// search it calls.
const DEPTH: usize = 2;

/// The name suffix of an owner's initializer.
const INITIALIZER: &str = "::PostInit()";

/// The name suffix of an object's validity check, which a list reader calls on each selected item.
const VALIDITY_CHECK: &str = "::IsValid() const";

/// The name part of a template database's handler for a key that it already holds.
const EXISTING_ENTRY: &str = "::ReadExistingEntry(CReader&, CString const&, ";

/// The signature suffix of a callee that an initializer's lookup can reach: a getter or a search
/// that takes the key string.
const KEY_SEARCH: &str = "(CString const&) const";

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

/// The verified executable and the facts that the catalog derived from it, as every static reader
/// reads them.
pub(in crate::binding) struct Image<'a> {
    pub bytes: &'a [u8],
    pub symbols: &'a [Symbol],
    pub strings: &'a BTreeMap<u64, String>,
    pub pointers: &'a BTreeMap<u64, u64>,
    pub imports: &'a BTreeMap<u64, String>,
    /// Every pointer slot that the loader binds to another image.
    pub bound_slots: &'a BTreeSet<u64>,
    /// The index of `symbols`.
    pub index: &'a super::symbols::SymbolIndex,
}

pub(in crate::binding) fn read(
    image: &Image<'_>,
    candidates: &[NamedCandidate],
) -> Result<ReferenceInput, AnalysisError> {
    let text = Text::read(image.bytes, image.symbols)?;
    let names = names(image.symbols, image.pointers, image.imports, image.strings);
    let named = |accept: fn(&str) -> bool| -> BTreeSet<String> {
        image
            .symbols
            .iter()
            .filter(|symbol| accept(&symbol.name))
            .map(|symbol| symbol.name.clone())
            .collect()
    };
    let readers = named(|name| reader(name).is_some());
    let initializers = named(|name| name.ends_with(INITIALIZER));
    let validity_checks = named(|name| name.ends_with(VALIDITY_CHECK));
    let mut lookups = readers.clone();
    lookups.insert(TRIGGER_LOOKUP.to_owned());
    let mut functions = reachable_functions(&text, image.symbols, &names, &lookups, |_| true);
    let initializer_functions =
        reachable_functions(&text, image.symbols, &names, &initializers, |callee| {
            callee.ends_with(KEY_SEARCH)
        });
    let databases = named_databases(&readers, &initializers, &initializer_functions, &names);
    let directories = database_directories(&text, image, candidates, databases);
    functions.extend(initializer_functions);
    functions.extend(reachable_functions(
        &text,
        image.symbols,
        &names,
        &validity_checks,
        |_| false,
    ));

    Ok(ReferenceInput {
        readers,
        initializers,
        functions,
        names,
        directories,
    })
}

/// The databases that the readers' signatures and the initializers' bodies name.
fn named_databases(
    readers: &BTreeSet<String>,
    initializers: &BTreeSet<String>,
    functions: &BTreeMap<String, Vec<Instruction>>,
    names: &BTreeMap<u64, String>,
) -> BTreeSet<String> {
    let mut databases: BTreeSet<String> = readers
        .iter()
        .filter_map(|callee| Some(reader(callee)?.database.to_owned()))
        .collect();
    for rows in initializers.iter().filter_map(|name| functions.get(name)) {
        let lines = canonical(rows, names);
        databases.extend(
            initialization::databases(&lines)
                .into_iter()
                .map(str::to_owned),
        );
    }

    databases
}

/// The decoded bodies of `roots` and of the functions that they reach within `DEPTH` calls,
/// following only callees that `follow` accepts.
fn reachable_functions(
    text: &Text<'_>,
    symbols: &[Symbol],
    names: &BTreeMap<u64, String>,
    roots: &BTreeSet<String>,
    follow: fn(&str) -> bool,
) -> BTreeMap<String, Vec<Instruction>> {
    let addresses = unique_addresses(symbols);
    let mut functions = BTreeMap::new();
    let mut pending: Vec<String> = roots.iter().cloned().collect();
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
            let callees = reached_functions(&canonical(&rows, names));
            reached.extend(callees.into_iter().filter(|callee| follow(callee)));
            functions.insert(name, rows);
        }
        pending = reached;
    }

    functions
}

/// The content directory of each database: the template join first, then the directory that the
/// database's own loader enumerates.
pub(in crate::binding) fn database_directories(
    text: &Text<'_>,
    image: &Image<'_>,
    candidates: &[NamedCandidate],
    databases: BTreeSet<String>,
) -> BTreeMap<String, Directory> {
    let anchors = super::constructors::anchors(image.symbols);

    databases
        .into_iter()
        .map(|database| {
            let directory = template_directory(candidates, &database).unwrap_or_else(|| {
                directories::loader_directory(
                    &own_functions(text, image.symbols, &database),
                    &instance_members(text, image, &database, &anchors),
                    &anchors,
                    image.strings,
                    image.pointers,
                )
            });
            (database, directory)
        })
        .collect()
}

/// The classes outside `known`, the template and scheduled databases, that load a directory on
/// their own: a function of the class calls the file enumeration, another or the same reads
/// statements, and the enumeration names a directory. Validated assumption: such a class reads
/// its directory's files as script.
pub(in crate::binding) fn unscheduled_loaders(
    text: &Text<'_>,
    image: &Image<'_>,
    known: &BTreeSet<&str>,
) -> BTreeMap<String, Directory> {
    let anchors = super::constructors::anchors(image.symbols);
    let symbols = image.index.over(image.symbols);
    let statement_reads: BTreeSet<u64> = symbols
        .named(super::fields::STATEMENT_READER)
        .map(|symbol| symbol.address)
        .collect();
    let mut enumerating = BTreeSet::new();
    let mut reading = BTreeSet::new();
    for (site, target) in text.calls_into(&(&anchors.file_enumerations | &statement_reads)) {
        let Some(&start) = text.starts.range(..=site).next_back() else {
            continue;
        };
        let start = BTreeSet::from([start]);
        let classes = symbols
            .at(&start)
            .filter_map(|symbol| method_class(&symbol.name));
        if anchors.file_enumerations.contains(&target) {
            enumerating.extend(classes);
        } else {
            reading.extend(classes);
        }
    }

    enumerating
        .intersection(&reading)
        .copied()
        .filter(|class| !known.contains(class))
        .filter_map(|class| {
            let directory = directories::loader_directory(
                &own_functions(text, image.symbols, class),
                &instance_members(text, image, class, &anchors),
                &anchors,
                image.strings,
                image.pointers,
            );

            (directory != Directory::Missing).then(|| (class.to_owned(), directory))
        })
        .collect()
}

/// The class that qualifies a method name, such as `CRandomNameDatabase` in
/// `CRandomNameDatabase::FillDatabase()`. `None` for a free function and for a template or
/// anonymous qualifier.
fn method_class(name: &str) -> Option<&str> {
    let (qualified, _) = name.split_once('(')?;
    let (class, _) = qualified.rsplit_once("::")?;
    let plain = class
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == ':');

    plain.then_some(class)
}

/// The arguments of every file enumeration in the functions of `classes`, the classes that load
/// one registry.
pub(in crate::binding) fn loader_enumerations(
    text: &Text<'_>,
    image: &Image<'_>,
    classes: &BTreeSet<String>,
) -> Vec<loader_rules::Enumeration> {
    let anchors = super::constructors::anchors(image.symbols);
    let functions: Vec<Constructor> = classes
        .iter()
        .flat_map(|class| own_functions(text, image.symbols, class))
        .collect();

    loader_rules::enumerations(&functions, &anchors, image.strings)
}

/// The duplicate rule of each template database in `databases`, from the one existing-entry
/// handler in its primary vtable. `None` when the vtable has no such handler or several, or
/// [`loader_rules::existing_entry_rule`] does not read the handler's body.
pub(in crate::binding) fn existing_entry_rules(
    text: &Text<'_>,
    image: &Image<'_>,
    databases: &BTreeSet<String>,
) -> Vec<Option<crate::DuplicateRule>> {
    let symbols = image.index.over(image.symbols);
    let handler_of = |database: &String| {
        let vtables: Vec<u64> = symbols
            .named(&format!("vtable for {database}"))
            .map(|symbol| symbol.address)
            .collect();
        let &[vtable] = vtables.as_slice() else {
            return None;
        };
        let handlers: BTreeSet<u64> = primary_slots(image, vtable)
            .filter(|&target| {
                symbols
                    .at(&BTreeSet::from([target]))
                    .any(|symbol| symbol.name.contains(EXISTING_ENTRY))
            })
            .collect();

        (handlers.len() == 1).then(|| *handlers.first().expect("one handler"))
    };

    databases
        .iter()
        .map(|database| {
            let (address, code) = text.function(handler_of(database)?).ok()?;

            loader_rules::existing_entry_rule(code, address)
        })
        .collect()
}

/// The targets of the primary vtable that starts at `start`: the pointer slots after its offset to
/// the top and its type information, up to the first slot that holds no pointer.
fn primary_slots<'a>(image: &'a Image<'_>, start: u64) -> impl Iterator<Item = u64> + 'a {
    (start + 16..)
        .step_by(8)
        .map_while(|slot| image.pointers.get(&slot).copied())
}

/// The functions that a canonical body calls, and the call operators of the resolver lambdas
/// whose vtables it names.
fn reached_functions(lines: &[Line]) -> Vec<String> {
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

/// The text of each `CString` member that the constructors of `class` and its
/// `CreateInstance()` build from a literal, by member offset.
pub(super) fn instance_members(
    text: &Text<'_>,
    image: &Image<'_>,
    class: &str,
    anchors: &directories::Anchors,
) -> BTreeMap<u64, String> {
    let symbols = image.index.over(image.symbols);
    let creator = format!("{class}::CreateInstance()");
    let bodies = |found: Vec<&Symbol>| -> Vec<Constructor> {
        found
            .into_iter()
            .filter_map(|symbol| {
                let (address, code) = text.function(symbol.address).ok()?;

                Some(Constructor {
                    address,
                    code: code.to_vec(),
                })
            })
            .collect()
    };

    directories::member_texts(
        &bodies(symbols.constructors_of(class)),
        &bodies(symbols.named(&creator).collect()),
        anchors,
        image.strings,
    )
}

/// Every function whose name is qualified by `class`.
pub(super) fn own_functions(text: &Text<'_>, symbols: &[Symbol], class: &str) -> Vec<Constructor> {
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
