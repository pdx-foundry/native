//! Reference lookups: the collection that the engine looks a value up in, when, and what a key
//! that names nothing yields.
//!
//! A reference reader's signature names its database, but not its lookup: one instantiation
//! scans the database's items while another calls a hash map. So each fact comes from a join of
//! its own. The database comes from the reader signature. The lookup semantics come only from a
//! complete-function [`shapes`] match of the reader, of the resolver lambda it registers, and of
//! the scan or map search that those call. The content directory comes from the binding's
//! directory join, never from the database's class name. A fact that no join establishes stays
//! unresolved with its reason.
pub mod initialization;
pub mod shapes;

use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::directories::Directory;
use crate::engine::analysis::stop::Unresolved;
use initialization::Initialization;
use shapes::{Bindings, Line, Shape, canonical};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

/// How a reference reader looks its key up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderForm {
    /// `ReadKeyReferenceDeferred<D>`: registers the key with the deferred resolver.
    Deferred,
    /// `ReadKeyReferenceDeferredUniform<D>`: a deferred list of keys.
    DeferredList,
    /// `ReadKeyReference<D>`: looks the key up while reading and returns the item.
    Immediate,
    /// `ReadKeyReferenceUniform<D, …>`: a list of keys looked up while reading.
    ImmediateList,
    /// `ReadIndexReferenceDeferred<D>`: a deferred index, not a key.
    DeferredIndex,
}

/// A reference reader and the database that its signature names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceReader<'a> {
    /// The database template argument.
    pub database: &'a str,
    /// The reader's lookup form.
    pub form: ReaderForm,
}

/// The reference reader that `callee` names, when its whole signature has a supported form.
pub fn reader(callee: &str) -> Option<ReferenceReader<'_>> {
    const DEFERRED: &str = "(CGlobalDeferredDatabaseObject const&, CReader&, ";

    if let Some((database, rest)) = template(callee, "void NParserUtil::ReadKeyReferenceDeferred<")
    {
        let expected = format!("{DEFERRED}{database}::ValueType const**)");
        return (rest == expected).then_some(ReferenceReader {
            database,
            form: ReaderForm::Deferred,
        });
    }
    if let Some((database, rest)) =
        template(callee, "void NParserUtil::ReadKeyReferenceDeferredUniform<")
    {
        return rest.starts_with(DEFERRED).then_some(ReferenceReader {
            database,
            form: ReaderForm::DeferredList,
        });
    }
    if let Some((database, rest)) =
        template(callee, "void NParserUtil::ReadIndexReferenceDeferred<")
    {
        return rest.starts_with(DEFERRED).then_some(ReferenceReader {
            database,
            form: ReaderForm::DeferredIndex,
        });
    }
    if let Some((prefix, rest)) =
        callee.split_once("::ValueType const* NParserUtil::ReadKeyReference<")
        && let Some((database, rest)) = rest.split_once('>')
        && prefix == database
        && is_template_argument(database)
    {
        let expected = format!("(CReader&, {database} const&, bool)");
        return (rest == expected).then_some(ReferenceReader {
            database,
            form: ReaderForm::Immediate,
        });
    }
    if let Some((database, rest)) = template_list(callee) {
        return rest.starts_with("(CReader&, ").then_some(ReferenceReader {
            database,
            form: ReaderForm::ImmediateList,
        });
    }

    None
}

/// The first template argument after `prefix`, and the text after the closing `>`.
fn template<'a>(callee: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    let rest = callee.strip_prefix(prefix)?;
    let (database, rest) = rest.split_once('>')?;

    is_template_argument(database).then_some((database, rest))
}

/// `ReadKeyReferenceUniform<D, CPdxArray<…>>` has a second template argument.
fn template_list(callee: &str) -> Option<(&str, &str)> {
    let rest = callee.strip_prefix("void NParserUtil::ReadKeyReferenceUniform<")?;
    let (database, rest) = rest.split_once(", ")?;
    let template_end = rest.find(">(")?;
    let parameters = &rest[template_end + 1..];

    is_template_argument(database).then_some((database, parameters))
}

/// A nonempty template argument of ASCII letters, digits and `_`.
fn is_template_argument(argument: &str) -> bool {
    !argument.is_empty()
        && argument
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// When the engine looks the key up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Inside the reader call.
    WhileReading,
    /// When the deferred resolver runs the registered lookup.
    Deferred,
    /// When the owner's initializer runs, after the owner is read.
    OwnerInitialization,
}

/// Which item a key selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMatch {
    /// The item whose key has the same bytes.
    Equal,
    /// The first item, in collection order, whose key has the same bytes.
    FirstEqual,
}

/// The lookup semantics that complete shapes establish. `None` is an unresolved fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lookup {
    /// When the lookup runs.
    pub stage: Stage,
    /// Which item the key selects, when the search was qualified.
    pub key_match: Option<KeyMatch>,
    /// Whether an empty key is looked up like any other key.
    pub empty_key_looked_up: Option<bool>,
    /// Whether a key that selects no item yields the typed null object.
    pub missing_yields_null: Option<bool>,
}

/// Everything established about one reference reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderLookup {
    /// The database that the reader's signature names.
    pub database: String,
    /// The database's content directory, when the directory join names one.
    pub directory: Option<String>,
    /// The lookup, or why no shape established it.
    pub lookup: Result<Lookup, Unresolved>,
}

/// Reference facts for every reference reader that a method may join.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceFacts {
    /// Keyed by the reader's demangled callee.
    pub readers: BTreeMap<String, ReaderLookup>,
    /// Keyed by the initializer's demangled name.
    pub initializers: BTreeMap<String, Initialization>,
}

/// The executable code and joins that the reference method reads.
#[derive(Debug, Clone, Default)]
pub struct ReferenceInput {
    /// Every reference reader callee to analyze.
    pub readers: BTreeSet<String>,
    /// Every owner initializer (`{Owner}::PostInit()`) to analyze.
    pub initializers: BTreeSet<String>,
    /// Complete decoded bodies by demangled name: the readers, their resolver lambdas, the
    /// initializers, and the functions that those call.
    pub functions: BTreeMap<String, Vec<Instruction>>,
    /// Demangled names at symbol addresses and pointer slots.
    pub names: BTreeMap<u64, String>,
    /// Content directory join of each database that a reader or an initializer names.
    pub directories: BTreeMap<String, Directory>,
}

/// Establish the facts of every reader and initializer in `input`.
pub fn analyze(input: &ReferenceInput) -> ReferenceFacts {
    let method = Method { input };
    let readers = input
        .readers
        .iter()
        .filter_map(|callee| {
            let reference = reader(callee)?;
            let lookup = method.lookup(callee, reference);

            Some((
                callee.clone(),
                ReaderLookup {
                    database: reference.database.to_owned(),
                    directory: method.directory(reference.database),
                    lookup,
                },
            ))
        })
        .collect();
    let initializers = input
        .initializers
        .iter()
        .map(|name| (name.clone(), method.initialization(name)))
        .collect();

    ReferenceFacts {
        readers,
        initializers,
    }
}

/// The lambda call operator of the resolver function that `vtable` names.
pub fn lambda_operator(vtable: &str) -> Option<String> {
    let function = vtable.strip_prefix("vtable for ")?;

    function
        .starts_with("std::__1::__function::__func<")
        .then(|| format!("{function}::operator()(CString const&)"))
}

struct Method<'a> {
    input: &'a ReferenceInput,
}

/// The null-object global of an already-qualified deferred single-item lookup.
pub(crate) fn deferred_null(input: &ReferenceInput, callee: &str) -> Option<String> {
    let method = Method { input };
    let reader = reader(callee)?;
    method.deferred(callee, reader.database).ok()?;
    let registration = DEFERRED.matches(&method.lines(callee)?)?;
    let operator = lambda_operator(&registration["lambda"])?;
    let lambda = method.lines(&operator)?;
    if let Some(bindings) = LAMBDA_MAP.matches(&lambda) {
        return Some(bindings["null"].clone());
    }
    if let Some(bindings) = LAMBDA_FORWARD.matches(&lambda) {
        let scan = FORWARDED_SCAN.matches(&method.lines(&bindings["forwarded"])?)?;
        return Some(scan["null"].clone());
    }
    let bindings = LAMBDA_GETTER.matches(&lambda)?;
    let scan = GETTER_SCAN.matches(&method.lines(&bindings["getter"])?)?;
    Some(scan["null"].clone())
}

impl Method<'_> {
    fn lookup(&self, callee: &str, reader: ReferenceReader<'_>) -> Result<Lookup, Unresolved> {
        match reader.form {
            ReaderForm::Immediate => self.immediate(callee),
            ReaderForm::Deferred => self.deferred(callee, reader.database),
            ReaderForm::DeferredList | ReaderForm::ImmediateList | ReaderForm::DeferredIndex => {
                Err(Unresolved::new("reference-list-form"))
            }
        }
    }

    fn directory(&self, database: &str) -> Option<String> {
        match self.input.directories.get(database) {
            Some(Directory::Named(name)) => Some(name.clone()),
            _ => None,
        }
    }

    fn lines(&self, name: &str) -> Option<Vec<Line>> {
        let body = self.input.functions.get(name)?;

        Some(canonical(body, &self.input.names))
    }

    /// `ReadKeyReference<D>` looks the key up inside the call, in the database it is given.
    fn immediate(&self, callee: &str) -> Result<Lookup, Unresolved> {
        let lines = self
            .lines(callee)
            .ok_or_else(|| Unresolved::new("reference-reader-body"))?;
        let own_clone =
            |bindings: &Bindings| bindings["cold"] == format!("{callee} [clone .cold.1]");
        let search = if let Some(bindings) = IMMEDIATE_SCAN.matches(&lines) {
            (null_object(&bindings) && own_clone(&bindings)).then_some(Some(KeyMatch::FirstEqual))
        } else if let Some(bindings) = IMMEDIATE_MAP.matches(&lines) {
            (null_object(&bindings) && own_clone(&bindings)).then(|| self.map_find(&bindings))
        } else {
            None
        };
        let key_match = search.ok_or_else(|| Unresolved::new("reference-reader-shape"))?;

        Ok(Lookup {
            stage: Stage::WhileReading,
            key_match,
            empty_key_looked_up: Some(true),
            missing_yields_null: Some(true),
        })
    }

    /// `ReadKeyReferenceDeferred<D>` registers the reader's token text and a lambda for the same
    /// database; the lambda's search gives the lookup semantics.
    fn deferred(&self, callee: &str, database: &str) -> Result<Lookup, Unresolved> {
        let lines = self
            .lines(callee)
            .ok_or_else(|| Unresolved::new("reference-reader-body"))?;
        let registration = DEFERRED
            .matches(&lines)
            .ok_or_else(|| Unresolved::new("reference-reader-shape"))?;
        let vtable = &registration["lambda"];
        if !vtable.contains(&format!("ReadKeyReferenceDeferred<{database}>(")) {
            return Err(Unresolved::new("reference-lambda-database"));
        }
        let operator =
            lambda_operator(vtable).ok_or_else(|| Unresolved::new("reference-lambda"))?;
        let lambda = self
            .lines(&operator)
            .ok_or_else(|| Unresolved::new("reference-lambda-body"))?;
        let search = self
            .lambda_search(&lambda, database)
            .ok_or_else(|| Unresolved::new("reference-lambda-shape"))?;

        Ok(Lookup {
            stage: Stage::Deferred,
            key_match: search,
            empty_key_looked_up: Some(true),
            missing_yields_null: Some(true),
        })
    }

    /// The key match of a resolver lambda's search, or `None` when no lambda shape matches with
    /// this database. An unqualified map search gives `Some(None)`.
    fn lambda_search(&self, lambda: &[Line], database: &str) -> Option<Option<KeyMatch>> {
        let instance = format!("TGameDatabase<{database}>::_pInstance");
        if let Some(bindings) = LAMBDA_MAP.matches(lambda) {
            return (bindings["database"] == instance && null_object(&bindings))
                .then(|| self.map_find(&bindings));
        }
        if let Some(bindings) = LAMBDA_FORWARD.matches(lambda) {
            let forwarded = &bindings["forwarded"];
            if !forwarded.contains(&format!("ReadKeyReferenceDeferred<{database}>(")) {
                return None;
            }
            let scan = FORWARDED_SCAN.matches(&self.lines(forwarded)?)?;
            return (scan["database"] == instance && null_object(&scan))
                .then_some(Some(KeyMatch::FirstEqual));
        }
        if let Some(bindings) = LAMBDA_GETTER.matches(lambda) {
            let getter = &bindings["getter"];
            let own = bindings["database"] == format!("{database}::_pInstance")
                && getter.starts_with(&format!("{database}::"))
                && getter.ends_with("(CString const&) const");
            if !own {
                return None;
            }
            let scan = GETTER_SCAN.matches(&self.lines(getter)?)?;
            return (null_object(&scan) && string_array_layout(&scan))
                .then_some(Some(KeyMatch::FirstEqual));
        }

        None
    }

    /// `Equal` when the called map search matches the qualified hash-and-compare shape.
    fn map_find(&self, bindings: &Bindings) -> Option<KeyMatch> {
        let find = &bindings["find"];
        let qualified = find.contains("::Find<CString>(CString const&) const")
            && self
                .lines(find)
                .is_some_and(|lines| MAP_FIND.matches(&lines).is_some());

        qualified.then_some(KeyMatch::Equal)
    }
}

/// Whether the shape's miss selects a typed null object.
fn null_object(bindings: &Bindings) -> bool {
    bindings["null"].starts_with("TPdxNullObject<") && bindings["null"].ends_with(">::_pInstance")
}

/// Whether a scan's offsets agree with the engine's array and string layouts: the count follows
/// the item pointer by `0xc`. A scan that reads mismatched offsets is not comparing one key.
fn string_array_layout(bindings: &Bindings) -> bool {
    let (Some(items), Some(count), Some(key), Some(length), Some(flag)) = (
        offset(bindings, "items"),
        offset(bindings, "count"),
        offset(bindings, "key"),
        offset(bindings, "length"),
        offset(bindings, "flag"),
    ) else {
        return false;
    };

    count == items + 0xc && string_layout(key, length, flag)
}

/// Whether a string's length and flag byte follow the string at `0x8` and `0x17`, the engine's
/// string layout.
fn string_layout(string: i64, length: i64, flag: i64) -> bool {
    length == string + 0x8 && flag == string + 0x17
}

/// The hexadecimal offset that the placeholder `name` captured.
fn offset(bindings: &Bindings, name: &str) -> Option<i64> {
    let text = bindings.get(name)?.strip_prefix("0x")?;

    i64::from_str_radix(text, 16).ok()
}

static DEFERRED: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("references/shapes/deferred.shape")));
static IMMEDIATE_SCAN: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("references/shapes/immediate_scan.shape")));
static IMMEDIATE_MAP: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("references/shapes/immediate_map.shape")));
static LAMBDA_MAP: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("references/shapes/lambda_map.shape")));
static LAMBDA_FORWARD: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("references/shapes/lambda_forward.shape")));
static FORWARDED_SCAN: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("references/shapes/forwarded_scan.shape")));
static LAMBDA_GETTER: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("references/shapes/lambda_getter.shape")));
static GETTER_SCAN: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("references/shapes/getter_scan.shape")));
static MAP_FIND: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("references/shapes/map_find.shape")));

#[cfg(test)]
#[path = "references/tests.rs"]
mod tests;
