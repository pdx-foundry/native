//! Owner-initialization lookups: an owner's `PostInit()` that looks up a key string stored in the
//! owner and stores the selected item back in the owner.
//!
//! Each supported initializer is one complete-function shape: a linear scan of the database's
//! items, a map search, or a call to a database getter whose own body is a qualified scan or hash
//! search. A shape binds the owner offsets of the key string and of the stored item, so a field
//! whose reader stores its string at the key offset joins the lookup. Offsets that do not agree
//! with one string layout, and a null object whose type differs from the collection's stated
//! element type, reject the lookup. An initializer that names more than one database is not
//! split into fragments.
use super::shapes::{Bindings, Line, Shape};
use super::{KeyMatch, Lookup, Method, Stage, null_object, offset, string_layout};
use crate::engine::analysis::stop::Unresolved;
use std::collections::BTreeSet;
use std::sync::LazyLock;

/// What an owner's initializer establishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Initialization {
    /// The initializer names no database, so it makes no lookup that the method must establish.
    NoLookup,
    /// One lookup that a qualified shape established.
    Lookup(InitializationLookup),
    /// The initializer names a database, but no qualified shape established its lookup.
    Unresolved(Unresolved),
}

/// One owner-initialization lookup of the key string that the owner stores at `input`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializationLookup {
    /// The searched database.
    pub database: String,
    /// The database's content directory, when the directory join names one.
    pub directory: Option<String>,
    /// Owner offset of the key string.
    pub input: i64,
    /// Owner offset where the selected item or the null object is stored.
    pub output: i64,
    /// The lookup semantics that the shapes establish.
    pub lookup: Lookup,
}

/// The database that a global names: `D` in `TGameDatabase<D>::_pInstance` or in `D::_pInstance`.
/// A typed null object is not a database.
pub fn database(global: &str) -> Option<&str> {
    let instance = global.strip_suffix("::_pInstance")?;
    if instance.starts_with("TPdxNullObject<") {
        return None;
    }
    let database = instance
        .strip_prefix("TGameDatabase<")
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(instance);

    super::is_template_argument(database).then_some(database)
}

/// The distinct databases that a canonical body names.
pub fn databases(lines: &[Line]) -> BTreeSet<&str> {
    lines
        .iter()
        .filter_map(|line| database(line.value.as_deref()?))
        .collect()
}

/// A shape's lookup before the directory join.
struct Found {
    database: String,
    input: i64,
    output: i64,
    key_match: Option<KeyMatch>,
    empty_key_looked_up: bool,
}

impl Method<'_> {
    /// Establish the lookup of the initializer `name`.
    pub(super) fn initialization(&self, name: &str) -> Initialization {
        let Some(lines) = self.lines(name) else {
            return Initialization::Unresolved(Unresolved::new("initializer-body"));
        };
        let named = databases(&lines).len();
        if named == 0 {
            return Initialization::NoLookup;
        }
        let found = match self.initializer_shape(&lines) {
            Some(found) => found,
            None if named > 1 => Err(Unresolved::new("initializer-several-lookups")),
            None => Err(Unresolved::new("initializer-shape")),
        };

        match found {
            Ok(found) => Initialization::Lookup(InitializationLookup {
                directory: self.directory(&found.database),
                database: found.database,
                input: found.input,
                output: found.output,
                lookup: Lookup {
                    stage: Stage::OwnerInitialization,
                    key_match: found.key_match,
                    empty_key_looked_up: Some(found.empty_key_looked_up),
                    missing_yields_null: Some(true),
                },
            }),
            Err(stop) => Initialization::Unresolved(stop),
        }
    }

    /// The lookup of the first initializer shape that matches, or `None` when none does. A
    /// matched shape whose bindings fail a check gives the check's reason.
    fn initializer_shape(&self, lines: &[Line]) -> Option<Result<Found, Unresolved>> {
        if let Some(bindings) = INITIALIZER_SCAN.matches(lines) {
            return Some(scan(&bindings, true));
        }
        if let Some(bindings) = INITIALIZER_SCAN_NONEMPTY.matches(lines) {
            return Some(scan(&bindings, false));
        }
        if let Some(bindings) = INITIALIZER_MAP.matches(lines) {
            return Some(self.map(&bindings, true));
        }
        if let Some(bindings) = INITIALIZER_MAP_NONEMPTY.matches(lines) {
            return Some(self.map(&bindings, false));
        }
        if let Some(bindings) = INITIALIZER_GETTER.matches(lines) {
            return Some(self.getter(&bindings));
        }

        None
    }

    /// A map search of `TGameDatabase<D>` whose `Find` states the map's element type.
    fn map(&self, bindings: &Bindings, empty_key_looked_up: bool) -> Result<Found, Unresolved> {
        let database = game_database(bindings)?;
        let input = offset(bindings, "input").ok_or_else(layout)?;
        if !empty_key_looked_up && !key_string(bindings, input) {
            return Err(layout());
        }
        let element = map_element(&bindings["find"]);
        if !null_object(bindings) || element != null_type(bindings) {
            return Err(Unresolved::new("initializer-null-type"));
        }

        Ok(Found {
            database,
            input,
            output: offset(bindings, "output").ok_or_else(layout)?,
            key_match: self.map_find(bindings),
            empty_key_looked_up,
        })
    }

    /// A call to `D::getter(CString const&) const` on `D::_pInstance`, whose body is a qualified
    /// scan of its own items or a hash search with a null substitute.
    fn getter(&self, bindings: &Bindings) -> Result<Found, Unresolved> {
        let getter = &bindings["getter"];
        let database = database(&bindings["database"])
            .filter(|database| {
                bindings["database"] == format!("{database}::_pInstance")
                    && getter.starts_with(&format!("{database}::"))
                    && getter.ends_with("(CString const&) const")
            })
            .ok_or_else(|| Unresolved::new("initializer-getter"))?;
        let body = self
            .lines(getter)
            .ok_or_else(|| Unresolved::new("initializer-getter-body"))?;
        let key_match = self.getter_search(&body)?;

        Ok(Found {
            database: database.to_owned(),
            input: offset(bindings, "input").ok_or_else(layout)?,
            output: offset(bindings, "output").ok_or_else(layout)?,
            key_match,
            empty_key_looked_up: true,
        })
    }

    /// The key match of a getter body: a scan of the getter's own items, or a hash search whose
    /// element type is the null object's type. An unqualified hash search gives `None`.
    fn getter_search(&self, body: &[Line]) -> Result<Option<KeyMatch>, Unresolved> {
        if let Some(scan) = super::GETTER_SCAN.matches(body) {
            if !null_object(&scan) {
                return Err(no_null_object());
            }
            if !super::string_array_layout(&scan) {
                return Err(layout());
            }
            return Ok(Some(KeyMatch::FirstEqual));
        }
        let Some(search) = NULL_GETTER.matches(body) else {
            return Err(Unresolved::new("initializer-getter-shape"));
        };
        let element = hash_element(&search["find"]);
        if !null_object(&search) || element != null_type(&search) {
            return Err(Unresolved::new("initializer-null-type"));
        }
        let qualified = self
            .lines(&search["find"])
            .and_then(|lines| HASH_FIND.matches(&lines))
            .is_some_and(|find| item_key(&find));

        Ok(qualified.then_some(KeyMatch::Equal))
    }
}

/// A linear scan of `TGameDatabase<D>`: the first equal item wins.
fn scan(bindings: &Bindings, empty_key_looked_up: bool) -> Result<Found, Unresolved> {
    let database = game_database(bindings)?;
    let input = offset(bindings, "input").ok_or_else(layout)?;
    if !null_object(bindings) {
        return Err(no_null_object());
    }
    if !key_string(bindings, input) || !item_key(bindings) {
        return Err(layout());
    }

    Ok(Found {
        database,
        input,
        output: offset(bindings, "output").ok_or_else(layout)?,
        key_match: Some(KeyMatch::FirstEqual),
        empty_key_looked_up,
    })
}

/// The database of a `TGameDatabase<D>::_pInstance` binding.
fn game_database(bindings: &Bindings) -> Result<String, Unresolved> {
    let global = &bindings["database"];
    let database = database(global)
        .filter(|database| *global == format!("TGameDatabase<{database}>::_pInstance"))
        .ok_or_else(|| Unresolved::new("initializer-collection"))?;

    Ok(database.to_owned())
}

/// Whether the owner's key string has one string layout at `input`.
fn key_string(bindings: &Bindings, input: i64) -> bool {
    let (Some(length), Some(flag)) = (
        offset(bindings, "input_length"),
        offset(bindings, "input_flag"),
    ) else {
        return false;
    };

    string_layout(input, length, flag)
}

/// Whether each item's key string has one string layout.
fn item_key(bindings: &Bindings) -> bool {
    let (Some(key), Some(length), Some(flag)) = (
        offset(bindings, "key"),
        offset(bindings, "length"),
        offset(bindings, "flag"),
    ) else {
        return false;
    };

    string_layout(key, length, flag)
}

fn layout() -> Unresolved {
    Unresolved::new("initializer-string-layout")
}

fn no_null_object() -> Unresolved {
    Unresolved::new("initializer-null-object")
}

/// `C` in the null object `TPdxNullObject<C>::_pInstance`.
fn null_type(bindings: &Bindings) -> Option<&str> {
    bindings["null"]
        .strip_prefix("TPdxNullObject<")?
        .strip_suffix(">::_pInstance")
}

/// `T` in a map `Find` over `CPdxUnorderedMap<CString, T const*, …>`.
fn map_element(find: &str) -> Option<&str> {
    let (_, rest) = find.split_once("CPdxUnorderedMap<CString, ")?;

    Some(rest.split_once(" const*, ")?.0)
}

/// `E` in `CHashTable<CString, E, …>::Find(CString const&) const`.
fn hash_element(find: &str) -> Option<&str> {
    let rest = find.strip_prefix("CHashTable<CString, ")?;
    if !find.ends_with(">::Find(CString const&) const") {
        return None;
    }

    Some(rest.split_once(", ")?.0)
}

static INITIALIZER_SCAN: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/initializer_scan.shape")));
static INITIALIZER_SCAN_NONEMPTY: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/initializer_scan_nonempty.shape")));
static INITIALIZER_MAP: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/initializer_map.shape")));
static INITIALIZER_MAP_NONEMPTY: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/initializer_map_nonempty.shape")));
static INITIALIZER_GETTER: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/initializer_getter.shape")));
static NULL_GETTER: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/null_getter.shape")));
static HASH_FIND: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/hash_find.shape")));

#[cfg(test)]
mod tests;
