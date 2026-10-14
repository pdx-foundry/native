//! Owner-initialization lookups: an owner's `PostInit()` that looks up a key string stored in the
//! owner and stores the selected item back in the owner.
//!
//! Each supported initializer is one complete-function shape: a linear scan of the database's
//! items, a map search, a call to a database getter whose own body is a qualified search, or a
//! scan whose selected item must pass its validity check to be stored. One shape holds two
//! lookups, each with its own key offset. A shape binds the owner offsets of the key string and of
//! the stored item, so a field whose reader stores its string at the key offset joins the lookup.
//! Offsets that do not agree with one string layout, and a null object whose type differs from
//! the collection's stated element type, reject the lookup. An initializer that names more than
//! one database and matches no shape is not split into fragments.
use super::shapes::{Bindings, Line, Shape};
use super::{KeyMatch, Lookup, Method, Missing, Stage, null_object, offset, string_layout};
use crate::engine::analysis::stop::Unresolved;
use std::collections::BTreeSet;
use std::sync::LazyLock;

/// What an owner's initializer establishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Initialization {
    /// The initializer names no database, so it makes no lookup that the method must establish.
    NoLookup,
    /// The lookups that a qualified shape established, one for each key string that the
    /// initializer looks up.
    Lookups(Vec<InitializationLookup>),
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
    pub key_offset: i64,
    /// Owner offset where the selected item or the null object is stored.
    pub item_offset: i64,
    /// The lookup semantics that the shapes establish.
    pub lookup: Lookup,
    /// Why the shape leaves the key match or the missing result unestablished, when it does.
    pub unstated: Option<&'static str>,
    /// The names that the matched shape bound, when the lookup is the initializer's whole effect
    /// and a key that selects no item stores a typed null object.
    pub execution: Option<Execution>,
}

/// The executable names that the matched shape bound, for the binding of an initializer stage
/// to addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Execution {
    /// A complete inline lookup with no effects outside the matched lookup.
    Inline { null: String },
    /// A complete initializer that calls this established getter.
    Getter {
        database: String,
        getter: String,
        null: String,
    },
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

/// A key that selects no item stores a null pointer, which `Missing` cannot name.
const NULL_POINTER: &str = "a key that selects no item stores a null pointer";

/// `CEventManager::GetEvent` selects by event id, not by the key's bytes.
const EVENT_ID: &str = "the getter selects the event by the id that \
    `CEventManager::GetIDInNameSpace` parses from the key, not by the key's bytes, and a key that \
    selects no event stores a null pointer";

/// The species-rights triggers also flag one keyword key.
const KEYWORD_FLAG: &str = "a key that selects no item stores a null pointer, and the \
    initializer then stores whether the key is the token `0x3ff8`";

/// A shape's lookup before the directory join.
struct Found {
    database: String,
    key_offset: i64,
    item_offset: i64,
    key_match: Option<KeyMatch>,
    empty_key_looked_up: bool,
    on_missing: Option<Missing>,
    unstated: Option<&'static str>,
    execution: Option<Execution>,
}

impl Method<'_> {
    /// Establish the lookups of the initializer `name`.
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
            Ok(found) => Initialization::Lookups(
                found
                    .into_iter()
                    .map(|found| self.initialization_lookup(found))
                    .collect(),
            ),
            Err(stop) => Initialization::Unresolved(stop),
        }
    }

    fn initialization_lookup(&self, found: Found) -> InitializationLookup {
        InitializationLookup {
            directory: self.directory(&found.database),
            database: found.database,
            key_offset: found.key_offset,
            item_offset: found.item_offset,
            lookup: Lookup {
                stage: Stage::OwnerInitialization,
                key_match: found.key_match,
                empty_key_looked_up: found.empty_key_looked_up,
                on_missing: found.on_missing,
            },
            unstated: found.unstated,
            execution: found.execution,
        }
    }

    /// The lookups of the first initializer shape that matches, or `None` when none does. A
    /// matched shape whose bindings fail a check gives the check's reason.
    fn initializer_shape(&self, lines: &[Line]) -> Option<Result<Vec<Found>, Unresolved>> {
        let one = |found: Result<Found, Unresolved>| Some(found.map(|found| vec![found]));

        if let Some(bindings) = INITIALIZER_SCAN.matches(lines) {
            return one(scan(&bindings, true));
        }
        if let Some(bindings) = INITIALIZER_SCAN_NONEMPTY.matches(lines) {
            return one(scan(&bindings, false));
        }
        if let Some(bindings) = INITIALIZER_MAP.matches(lines) {
            return one(self.map(&bindings, true));
        }
        if let Some(bindings) = INITIALIZER_MAP_NONEMPTY.matches(lines) {
            return one(self.map(&bindings, false));
        }
        if let Some(bindings) = INITIALIZER_GETTER.matches(lines) {
            return one(GetterCall::bound(&bindings, "").and_then(|call| self.getter(&call)));
        }
        if let Some(bindings) = INITIALIZER_SCAN_VALID.matches(lines) {
            return one(self.scan_valid(&bindings, NULL_POINTER));
        }
        if let Some(bindings) = INITIALIZER_SCAN_VALID_FLAG.matches(lines) {
            return one(self.scan_valid(&bindings, KEYWORD_FLAG));
        }
        if let Some(bindings) = INITIALIZER_GETTER_SCAN_NONEMPTY.matches(lines) {
            return Some(self.getter_then_scan(&bindings));
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
            key_offset: input,
            item_offset: offset(bindings, "output").ok_or_else(layout)?,
            key_match: self.map_find(bindings),
            empty_key_looked_up,
            on_missing: Some(Missing::NullObject),
            unstated: None,
            execution: Some(Execution::Inline {
                null: bindings["null"].clone(),
            }),
        })
    }

    /// A call to `D::getter(CString const&) const` on `D::_pInstance`. The getter's body is a
    /// qualified scan or hash search with a null substitute, or a search that yields a null
    /// pointer for a key that selects nothing.
    fn getter(&self, call: &GetterCall<'_>) -> Result<Found, Unresolved> {
        let getter = call.getter;
        let database = database(call.database)
            .filter(|database| {
                call.database == format!("{database}::_pInstance")
                    && getter.starts_with(&format!("{database}::"))
                    && getter.ends_with("(CString const&) const")
            })
            .ok_or_else(|| Unresolved::new("initializer-getter"))?;
        let body = self
            .lines(getter)
            .ok_or_else(|| Unresolved::new("initializer-getter-body"))?;
        let found = |key_match, on_missing, unstated, execution| Found {
            database: database.to_owned(),
            key_offset: call.input,
            item_offset: call.output,
            key_match,
            empty_key_looked_up: true,
            on_missing,
            unstated,
            execution,
        };

        match self.getter_search(&body) {
            Ok((key_match, null)) => {
                let execution = Execution::Getter {
                    database: call.database.to_owned(),
                    getter: getter.to_owned(),
                    null,
                };
                Ok(found(
                    key_match,
                    Some(Missing::NullObject),
                    None,
                    Some(execution),
                ))
            }
            Err(_) if EVENT_GETTER.matches(&body).is_some() => {
                Ok(found(None, None, Some(EVENT_ID), None))
            }
            Err(stop) => {
                let search = HASH_GETTER
                    .matches(&body)
                    .filter(|search| hash_element(&search["find"]).is_some())
                    .ok_or(stop)?;
                let key_match = self.hash_find(&search["find"]);

                Ok(found(key_match, None, Some(NULL_POINTER), None))
            }
        }
    }

    /// A linear scan whose selected object is stored only when its validity check passes. The
    /// check of the scanned item type returns 1 and the null object's returns 0, so a key that
    /// selects no item stores a null pointer; `unstated` says so, with anything else the miss does.
    fn scan_valid(&self, bindings: &Bindings, unstated: &'static str) -> Result<Found, Unresolved> {
        let item = null_type(bindings).ok_or_else(no_null_object)?;
        let checks = self.validity(item) == Some(true)
            && self.validity(&format!("TPdxNullObject<{item}>")) == Some(false);
        if !checks {
            return Err(Unresolved::new("initializer-validity"));
        }

        Ok(Found {
            on_missing: None,
            unstated: Some(unstated),
            ..partial_scan(bindings)?
        })
    }

    /// A getter call, then a scan that runs only for a nonempty key: two lookups of two keys. The
    /// initializer does both, so neither is its whole effect.
    fn getter_then_scan(&self, bindings: &Bindings) -> Result<Vec<Found>, Unresolved> {
        let call = GetterCall::bound(bindings, "call_")?;
        let getter = Found {
            execution: None,
            ..self.getter(&call)?
        };
        let scan = Found {
            empty_key_looked_up: false,
            ..partial_scan(bindings)?
        };

        Ok(vec![getter, scan])
    }

    /// The key match of a getter body and the null object that its miss yields: a scan of the
    /// getter's own items, or a hash search whose element type is the null object's type. An
    /// unqualified hash search has no key match.
    pub(super) fn getter_search(
        &self,
        body: &[Line],
    ) -> Result<(Option<KeyMatch>, String), Unresolved> {
        if let Some(scan) = super::GETTER_SCAN.matches(body) {
            if !null_object(&scan) {
                return Err(no_null_object());
            }
            if !super::string_array_layout(&scan) {
                return Err(layout());
            }
            return Ok((Some(KeyMatch::FirstEqual), scan["null"].clone()));
        }
        let Some(search) = NULL_GETTER.matches(body) else {
            return Err(Unresolved::new("initializer-getter-shape"));
        };
        let element = hash_element(&search["find"]);
        if !null_object(&search) || element != null_type(&search) {
            return Err(Unresolved::new("initializer-null-type"));
        }

        Ok((self.hash_find(&search["find"]), search["null"].clone()))
    }

    /// `Equal` when the hash table search `find` matches the qualified hash-and-compare shape.
    fn hash_find(&self, find: &str) -> Option<KeyMatch> {
        let qualified = self
            .lines(find)
            .and_then(|lines| HASH_FIND.matches(&lines))
            .is_some_and(|find| item_key(&find));

        qualified.then_some(KeyMatch::Equal)
    }
}

/// A getter call that an initializer shape bound under the placeholder prefix: the global it is
/// called on, the getter, and the owner offsets of the key string and of the stored result.
struct GetterCall<'a> {
    database: &'a str,
    getter: &'a str,
    input: i64,
    output: i64,
}

impl<'a> GetterCall<'a> {
    fn bound(bindings: &'a Bindings, prefix: &str) -> Result<Self, Unresolved> {
        let named = |name: &str| format!("{prefix}{name}");

        Ok(Self {
            database: &bindings[&named("database")],
            getter: &bindings[&named("getter")],
            input: offset(bindings, &named("input")).ok_or_else(layout)?,
            output: offset(bindings, &named("output")).ok_or_else(layout)?,
        })
    }
}

/// A linear scan of `TGameDatabase<D>`: the first equal item wins, and the lookup is the
/// initializer's whole effect.
fn scan(bindings: &Bindings, empty_key_looked_up: bool) -> Result<Found, Unresolved> {
    Ok(Found {
        empty_key_looked_up,
        execution: Some(Execution::Inline {
            null: bindings["null"].clone(),
        }),
        ..partial_scan(bindings)?
    })
}

/// A linear scan of `TGameDatabase<D>` that selects the first equal item or the typed null object,
/// in an initializer that does more than store the selection.
fn partial_scan(bindings: &Bindings) -> Result<Found, Unresolved> {
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
        key_offset: input,
        item_offset: offset(bindings, "output").ok_or_else(layout)?,
        key_match: Some(KeyMatch::FirstEqual),
        empty_key_looked_up: true,
        on_missing: Some(Missing::NullObject),
        unstated: None,
        execution: None,
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
pub(super) fn null_type(bindings: &Bindings) -> Option<&str> {
    bindings["null"]
        .strip_prefix("TPdxNullObject<")?
        .strip_suffix(">::_pInstance")
}

/// `T` in a map `Find` over `CPdxUnorderedMap<CString, T const*, …>`.
pub(super) fn map_element(find: &str) -> Option<&str> {
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
static INITIALIZER_SCAN_VALID: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/initializer_scan_valid.shape")));
static INITIALIZER_SCAN_VALID_FLAG: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/initializer_scan_valid_flag.shape")));
static INITIALIZER_GETTER_SCAN_NONEMPTY: LazyLock<Shape> = LazyLock::new(|| {
    Shape::parse(include_str!(
        "shapes/initializer_getter_scan_nonempty.shape"
    ))
});
static EVENT_GETTER: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/event_getter.shape")));
static HASH_GETTER: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/hash_getter.shape")));
static NULL_GETTER: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/null_getter.shape")));
static HASH_FIND: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("shapes/hash_find.shape")));

#[cfg(test)]
mod tests;
