use super::*;
use crate::engine::analysis::assembler::{Arm64, arm64};
use crate::engine::analysis::decode::decode_arm64;

const SHIP: &str = "void NParserUtil::ReadKeyReferenceDeferred<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CShipDatabase::ValueType const**)";
const SHIP_IMMEDIATE: &str = "CShipDatabase::ValueType const* NParserUtil::ReadKeyReference<CShipDatabase>(CReader&, CShipDatabase const&, bool)";
const LAMBDA_VTABLE: &str = "vtable for std::__1::__function::__func<void NParserUtil::ReadKeyReferenceDeferred<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CString const&, CString const&, CShipDatabase::ValueType const**)::{lambda(CString const&)#1}>";
const FORWARDED: &str = "bool std::__1::__invoke_void_return_wrapper<bool, false>::__call<void NParserUtil::ReadKeyReferenceDeferred<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CString const&, CString const&, CShipDatabase::ValueType const**)::{lambda(CString const&)#1}&, CString const&>(CShipDatabase::ValueType const**)";
const FIND: &str = "NPdxRobinHoodTable::CIterator<CShip> CPdxRobinHoodTable<CShip>::Find<CString>(CString const&) const";
const GETTER: &str = "CShipDatabase::GetShip(CString const&) const";

/// Code and names built from shape text: each placeholder is replaced by a test value, scratch
/// registers `xrN` become `x(8+N)`, and every named address gets its own location. Each unnamed
/// `?` address gets its own `0x100`-byte range, recorded in order, so data can be placed there.
#[derive(Default)]
pub(crate) struct Image {
    functions: BTreeMap<String, Vec<Instruction>>,
    names: BTreeMap<u64, String>,
    next_function: u64,
    unnamed: Vec<u64>,
}

impl Image {
    fn address_of(&mut self, name: &str) -> u64 {
        if let Some((address, _)) = self.names.iter().find(|(_, known)| *known == name) {
            return *address;
        }
        let address = 0x80_0000 + self.names.len() as u64 * 0x18;
        self.names.insert(address, name.to_owned());

        address
    }

    /// The address that a shape value names: a fresh unnamed range for `?`.
    fn value_address(&mut self, value: &str) -> u64 {
        if value != "?" {
            return self.address_of(value);
        }

        let address = 0x90_0000 + self.unnamed.len() as u64 * 0x100;
        self.unnamed.push(address);

        address
    }

    /// Name `class`'s vtable and give its validity check (slot `0x10`) the body that returns
    /// `value`.
    pub(crate) fn validity(&mut self, class: &str, value: &str) -> &mut Self {
        let vtable = 0xa0_0000 + self.names.len() as u64 * 0x40;
        let check = format!("{class}::IsValid() const");
        self.names.insert(vtable, format!("vtable for {class}"));
        self.names.insert(vtable + 0x20, check.clone());

        self.add(&check, shape("valid"), &[("value", value)])
    }

    /// The rows of the function that `add` built as `name`.
    pub(crate) fn function(&self, name: &str) -> &[Instruction] {
        &self.functions[name]
    }

    pub(crate) fn names(&self) -> &BTreeMap<u64, String> {
        &self.names
    }

    /// The unnamed `?` addresses, in the order that their `adrp` lines formed them.
    pub(crate) fn unnamed(&self) -> &[u64] {
        &self.unnamed
    }

    /// Add the function `name`, built from `shape` with `values` substituted for placeholders.
    pub(crate) fn add(&mut self, name: &str, shape: &str, values: &[(&str, &str)]) -> &mut Self {
        let mut text = shape.to_owned();
        for (key, value) in values {
            text = text.replace(&format!("{{{key}}}"), value);
        }
        let lines: Vec<(String, Option<String>)> = text
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| match line.split_once(" = ") {
                Some((text, value)) => (text.to_owned(), Some(value.to_owned())),
                None => (line.to_owned(), None),
            })
            .collect();
        self.next_function += 0x1_0000;
        let start = self.next_function;
        let rows = lines
            .iter()
            .enumerate()
            .map(|(index, (text, value))| {
                let address = start + index as u64 * 4;
                let next_value = lines[index + 1..]
                    .iter()
                    .find(|(text, _)| text.contains(",G"))
                    .and_then(|(_, value)| value.clone());
                self.row(address, text, value.as_deref(), next_value.as_deref())
            })
            .collect();
        self.functions.insert(name.to_owned(), rows);

        self
    }

    fn row(
        &mut self,
        address: u64,
        text: &str,
        value: Option<&str>,
        next: Option<&str>,
    ) -> Instruction {
        let (operation, operands) = text.split_once(' ').unwrap_or((text, ""));
        let mut operands = concrete_registers(operands);
        if operation == "adrp" {
            let address = self.value_address(next.expect("an adrp completes a named address"));
            let page = address & !0xfff;
            operands = operands.replace("PAGE", &format!("#{page:#x}"));
        } else if let Some(value) = value.filter(|_| operands.contains('G')) {
            let address = match value {
                "?" => *self
                    .unnamed
                    .last()
                    .expect("an adrp forms each unnamed address"),
                named => self.address_of(named),
            };
            let offset = address & 0xfff;
            operands = operands.replacen('G', &format!("#{offset:#x}"), 1);
        } else if let Some(value) = value.filter(|_| operands.ends_with("CALL")) {
            let target = self.address_of(value);
            operands = operands.replace("CALL", &format!("#{target:#x}"));
        } else if let Some((before, relative)) = operands.rsplit_once('@') {
            let target = address as i64 + relative.parse::<i64>().unwrap() * 4;
            operands = format!("{before}#{target:#x}");
        }

        Instruction {
            address,
            bytes: [0; 4],
            operation: operation.into(),
            operands,
        }
    }

    pub(super) fn input(
        &self,
        readers: &[&str],
        directories: &[(&str, Directory)],
    ) -> ReferenceInput {
        ReferenceInput {
            readers: readers.iter().map(|reader| (*reader).to_owned()).collect(),
            initializers: BTreeSet::new(),
            functions: self.functions.clone(),
            names: self.names.clone(),
            directories: directories
                .iter()
                .map(|(database, directory)| ((*database).to_owned(), directory.clone()))
                .collect(),
        }
    }
}

/// `xrN` and `wrN` become `x(8+N)` and `w(8+N)`, the registers that canonicalization renames.
fn concrete_registers(operands: &str) -> String {
    let mut result = String::new();
    let mut rest = operands;
    while let Some(at) = rest.find(['x', 'w']) {
        let (before, candidate) = rest.split_at(at);
        result.push_str(before);
        let digits: String = candidate[2..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if candidate[1..].starts_with('r') && !digits.is_empty() {
            let register = 8 + digits.parse::<u8>().unwrap();
            result.push_str(&format!("{}{register}", &candidate[..1]));
            rest = &candidate[2 + digits.len()..];
        } else {
            result.push_str(&candidate[..1]);
            rest = &candidate[1..];
        }
    }
    result.push_str(rest);

    result
}

pub(super) fn shape(name: &str) -> &'static str {
    match name {
        "initializer_scan" => include_str!("shapes/initializer_scan.shape"),
        "initializer_scan_nonempty" => include_str!("shapes/initializer_scan_nonempty.shape"),
        "initializer_map" => include_str!("shapes/initializer_map.shape"),
        "initializer_map_nonempty" => include_str!("shapes/initializer_map_nonempty.shape"),
        "initializer_getter" => include_str!("shapes/initializer_getter.shape"),
        "null_getter" => include_str!("shapes/null_getter.shape"),
        "hash_find" => include_str!("shapes/hash_find.shape"),
        "deferred" => include_str!("shapes/deferred.shape"),
        "immediate_scan" => include_str!("shapes/immediate_scan.shape"),
        "immediate_map" => include_str!("shapes/immediate_map.shape"),
        "lambda_map" => include_str!("shapes/lambda_map.shape"),
        "lambda_forward" => include_str!("shapes/lambda_forward.shape"),
        "forwarded_scan" => include_str!("shapes/forwarded_scan.shape"),
        "lambda_getter" => include_str!("shapes/lambda_getter.shape"),
        "getter_scan" => include_str!("shapes/getter_scan.shape"),
        "immediate_scan_base" => include_str!("shapes/immediate_scan_base.shape"),
        "immediate_getter" => include_str!("shapes/immediate_getter.shape"),
        "list_scan" => include_str!("shapes/list_scan.shape"),
        "list_map" => include_str!("shapes/list_map.shape"),
        "deferred_list" => include_str!("shapes/deferred_list.shape"),
        "forwarded_list_scan" => include_str!("shapes/forwarded_list_scan.shape"),
        "valid" => include_str!("shapes/valid.shape"),
        "map_find" => include_str!("shapes/map_find.shape"),
        "trigger_token" => include_str!("shapes/trigger_token.shape"),
        "trigger_map" => include_str!("shapes/trigger_map.shape"),
        _ => unreachable!("{name}"),
    }
}

fn operator() -> String {
    lambda_operator(LAMBDA_VTABLE).unwrap()
}

/// A deferred ship reader whose lambda forwards to a linear scan.
fn deferred_scan() -> Image {
    let mut image = Image::default();
    image
        .add(SHIP, shape("deferred"), &[("lambda", LAMBDA_VTABLE)])
        .add(
            &operator(),
            shape("lambda_forward"),
            &[("forwarded", FORWARDED)],
        )
        .add(
            FORWARDED,
            shape("forwarded_scan"),
            &[
                ("database", "TGameDatabase<CShipDatabase>::_pInstance"),
                ("null", "TPdxNullObject<CShip>::_pInstance"),
            ],
        );

    image
}

fn named() -> [(&'static str, Directory); 1] {
    [("CShipDatabase", Directory::Named("common/ships".into()))]
}

fn lookup(image: &Image, reader: &str) -> ReaderLookup {
    analyze(&image.input(&[reader], &named())).readers[reader].clone()
}

#[test]
fn reader_signatures_name_their_database_and_form() {
    let cases = [
        (SHIP, ReaderForm::Deferred),
        (SHIP_IMMEDIATE, ReaderForm::Immediate),
        (
            "void NParserUtil::ReadKeyReferenceDeferredUniform<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CPdxArray<CShipDatabase::ValueType const*, int>&)",
            ReaderForm::DeferredList,
        ),
        (
            "void NParserUtil::ReadKeyReferenceUniform<CShipDatabase, CPdxArray<CShip const*, int> >(CReader&, CShipDatabase const&, CPdxArray<CShip const*, int>&)",
            ReaderForm::ImmediateList,
        ),
        (
            "void NParserUtil::ReadIndexReferenceDeferred<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CShipDatabase::ValueType const**)",
            ReaderForm::DeferredIndex,
        ),
    ];
    for (callee, form) in cases {
        assert_eq!(
            reader(callee),
            Some(ReferenceReader {
                database: "CShipDatabase",
                form
            }),
            "{callee}"
        );
    }

    for callee in [
        "void NParserUtil::ReadKeyReferenceDeferred<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, COtherDatabase::ValueType const**)",
        "COtherDatabase::ValueType const* NParserUtil::ReadKeyReference<CShipDatabase>(CReader&, CShipDatabase const&, bool)",
        "void NParserUtil::ReadKeyReferenceDeferred<A<B>>(CGlobalDeferredDatabaseObject const&, CReader&, A<B>::ValueType const**)",
        "CReader::Read(CString&, bool)",
    ] {
        assert_eq!(reader(callee), None, "{callee}");
    }
}

#[test]
fn a_deferred_reader_with_a_forwarded_scan_is_a_deferred_first_equal_lookup() {
    let fact = lookup(&deferred_scan(), SHIP);

    assert_eq!(fact.database, "CShipDatabase");
    assert_eq!(fact.directory.as_deref(), Some("common/ships"));
    assert_eq!(
        fact.lookup,
        Ok(Lookup {
            stage: Stage::Deferred,
            key_match: Some(KeyMatch::FirstEqual),
            empty_key_looked_up: true,
            on_missing: Missing::NullObject,
        })
    );
    assert_eq!(
        fact.null_object.as_deref(),
        Some("TPdxNullObject<CShip>::_pInstance")
    );
}

#[test]
fn ownership_and_lookup_are_separate_joins() {
    let image = deferred_scan();
    let input = image.input(&[SHIP], &[("CShipDatabase", Directory::Missing)]);
    let fact = &analyze(&input).readers[SHIP];

    assert_eq!(fact.directory, None);
    assert!(fact.lookup.is_ok());

    let mut other = Image::default();
    other
        .add(SHIP, shape("deferred"), &[("lambda", LAMBDA_VTABLE)])
        .add(
            &operator(),
            shape("lambda_forward"),
            &[("forwarded", FORWARDED)],
        )
        .add(
            FORWARDED,
            shape("forwarded_scan"),
            &[
                ("database", "TGameDatabase<COtherDatabase>::_pInstance"),
                ("null", "TPdxNullObject<CShip>::_pInstance"),
            ],
        );
    let fact = lookup(&other, SHIP);

    assert_eq!(fact.directory.as_deref(), Some("common/ships"));
    assert_eq!(
        fact.lookup.unwrap_err().reason,
        "reference-lambda-shape",
        "a scan of another database is not this reader's lookup"
    );
}

#[test]
fn a_lambda_registered_for_another_database_is_not_joined() {
    let mut image = deferred_scan();
    let other = LAMBDA_VTABLE.replace("CShipDatabase", "COtherDatabase");
    image.add(SHIP, shape("deferred"), &[("lambda", &other)]);

    assert_eq!(
        lookup(&image, SHIP).lookup.unwrap_err().reason,
        "reference-lambda-database"
    );
}

#[test]
fn a_miss_that_selects_no_null_object_is_not_a_lookup() {
    let mut image = deferred_scan();
    image.add(
        FORWARDED,
        shape("forwarded_scan"),
        &[
            ("database", "TGameDatabase<CShipDatabase>::_pInstance"),
            ("null", "CShip::s_Default"),
        ],
    );

    let fact = lookup(&image, SHIP);
    assert_eq!(fact.null_object, None);
    assert_eq!(fact.lookup.unwrap_err().reason, "reference-lambda-shape");
}

#[test]
fn a_changed_comparison_callee_or_branch_breaks_the_scan() {
    let changed_callee = shape("forwarded_scan").replace("CALL = _memcmp", "CALL = _strlen");
    let changed_branch = shape("forwarded_scan").replacen("cbz w0,", "cbnz w0,", 1);
    for body in [changed_callee, changed_branch] {
        assert_ne!(body, shape("forwarded_scan"));
        let mut image = deferred_scan();
        image.add(
            FORWARDED,
            &body,
            &[
                ("database", "TGameDatabase<CShipDatabase>::_pInstance"),
                ("null", "TPdxNullObject<CShip>::_pInstance"),
            ],
        );

        assert!(lookup(&image, SHIP).lookup.is_err());
    }
}

#[test]
fn a_map_search_is_equal_only_when_its_find_is_qualified() {
    let mut image = Image::default();
    image
        .add(SHIP, shape("deferred"), &[("lambda", LAMBDA_VTABLE)])
        .add(
            &operator(),
            shape("lambda_map"),
            &[
                ("database", "TGameDatabase<CShipDatabase>::_pInstance"),
                ("find", FIND),
                ("null", "TPdxNullObject<CShip>::_pInstance"),
            ],
        );
    let unqualified = lookup(&image, SHIP);
    assert_eq!(unqualified.lookup.unwrap().key_match, None);
    assert_eq!(
        unqualified.null_object.as_deref(),
        Some("TPdxNullObject<CShip>::_pInstance")
    );

    image.add(FIND, shape("map_find"), &[]);
    let qualified = lookup(&image, SHIP).lookup.unwrap();
    assert_eq!(qualified.key_match, Some(KeyMatch::Equal));

    let hashless = shape("map_find").replace("CALL = _PMurHash32", "CALL = _rand");
    assert_ne!(hashless, shape("map_find"));
    image.add(FIND, &hashless, &[]);
    assert_eq!(lookup(&image, SHIP).lookup.unwrap().key_match, None);
}

#[test]
fn an_immediate_reader_looks_up_while_reading_and_names_its_own_clone() {
    let clone = format!("{SHIP_IMMEDIATE} [clone .cold.1]");
    let values = [
        ("null", "TPdxNullObject<CShip>::_pInstance"),
        ("source", "\"parser_util.h\""),
        ("cold", clone.as_str()),
    ];
    let mut image = Image::default();
    image.add(SHIP_IMMEDIATE, shape("immediate_scan"), &values);
    let fact = lookup(&image, SHIP_IMMEDIATE);
    assert_eq!(
        fact.lookup,
        Ok(Lookup {
            stage: Stage::WhileReading,
            key_match: Some(KeyMatch::FirstEqual),
            empty_key_looked_up: true,
            on_missing: Missing::NullObject,
        })
    );
    assert_eq!(
        fact.null_object.as_deref(),
        Some("TPdxNullObject<CShip>::_pInstance")
    );

    let foreign = [
        values[0],
        values[1],
        ("cold", "COther::Read() [clone .cold.1]"),
    ];
    image.add(SHIP_IMMEDIATE, shape("immediate_scan"), &foreign);
    assert_eq!(
        lookup(&image, SHIP_IMMEDIATE).lookup.unwrap_err().reason,
        "reference-reader-shape"
    );
}

#[test]
fn a_getter_scan_requires_one_consistent_key_layout() {
    let getter = |flag: &str| {
        let mut image = Image::default();
        image
            .add(SHIP, shape("deferred"), &[("lambda", LAMBDA_VTABLE)])
            .add(
                &operator(),
                shape("lambda_getter"),
                &[
                    ("database", "CShipDatabase::_pInstance"),
                    ("getter", GETTER),
                    ("validity", "0x98"),
                ],
            )
            .add(
                GETTER,
                shape("getter_scan"),
                &[
                    ("items", "0x3d0"),
                    ("count", "0x3dc"),
                    ("key", "0x158"),
                    ("length", "0x160"),
                    ("flag", flag),
                    ("null", "TPdxNullObject<CShip>::_pInstance"),
                ],
            );
        lookup(&image, SHIP)
    };

    let consistent = getter("0x16f");
    assert_eq!(
        consistent.lookup.unwrap().key_match,
        Some(KeyMatch::FirstEqual)
    );
    assert_eq!(
        consistent.null_object.as_deref(),
        Some("TPdxNullObject<CShip>::_pInstance")
    );
    assert!(getter("0x170").lookup.is_err());
}

#[test]
fn an_index_reader_keeps_its_lookup_unresolved() {
    let index = "void NParserUtil::ReadIndexReferenceDeferred<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CShipDatabase::ValueType const**)";
    let fact = lookup(&Image::default(), index);

    assert_eq!(fact.directory.as_deref(), Some("common/ships"));
    assert_eq!(fact.lookup.unwrap_err().reason, "reference-index-form");
}

const SHIP_LIST: &str = "void NParserUtil::ReadKeyReferenceUniform<CShipDatabase, CPdxArray<CShip const*, int> >(CReader&, CShipDatabase const&, CPdxArray<CShip const*, int>&)";
const SHIP_DEFERRED_LIST: &str = "void NParserUtil::ReadKeyReferenceDeferredUniform<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CPdxArray<CShipDatabase::ValueType const*, int>&)";
const SHIP_INSERT: &str = "CShip const*& CPdxArray<CShip const*, int>::InsertAtEmplace<CShip const* const&>(int, CShip const* const&)";
const OTHER_INSERT: &str = "COther const*& CPdxArray<COther const*, int>::InsertAtEmplace<COther const* const&>(int, COther const* const&)";
const SHIP_NULL: &str = "TPdxNullObject<CShip>::_pInstance";

/// An immediate ship list read by `shape` with `values`, a ship check that returns `item` and a
/// null-object check that returns `null`.
fn immediate_list(shape_name: &str, values: &[(&str, &str)], item: &str, null: &str) -> Image {
    let mut image = Image::default();
    image
        .add(SHIP_LIST, shape(shape_name), values)
        .validity("CShip", item)
        .validity("TPdxNullObject<CShip>", null);

    image
}

const LIST_SCAN_VALUES: &[(&str, &str)] = &[
    ("typeinfo", "typeinfo name for CShipDatabase"),
    ("insert", SHIP_INSERT),
    ("null", SHIP_NULL),
    ("source", "\"parser_util.h\""),
];

#[test]
fn an_immediate_list_skips_a_key_whose_null_object_fails_its_validity_check() {
    let fact = lookup(
        &immediate_list("list_scan", LIST_SCAN_VALUES, "0x1", "0x0"),
        SHIP_LIST,
    );

    assert_eq!(
        fact.lookup,
        Ok(Lookup {
            stage: Stage::WhileReading,
            key_match: Some(KeyMatch::FirstEqual),
            empty_key_looked_up: true,
            on_missing: Missing::Skipped,
        })
    );
    assert_eq!(fact.null_object.as_deref(), Some(SHIP_NULL));
}

#[test]
fn an_immediate_list_whose_validity_checks_do_not_split_hit_and_miss_is_unresolved() {
    for (item, null) in [("0x1", "0x1"), ("0x0", "0x0")] {
        let image = immediate_list("list_scan", LIST_SCAN_VALUES, item, null);

        assert_eq!(
            lookup(&image, SHIP_LIST).lookup.unwrap_err().reason,
            "reference-list-validity",
            "item {item}, null {null}"
        );
    }
}

#[test]
fn an_immediate_list_must_append_and_log_its_own_types() {
    let other_insert = [
        LIST_SCAN_VALUES[0],
        ("insert", OTHER_INSERT),
        LIST_SCAN_VALUES[2],
        LIST_SCAN_VALUES[3],
    ];
    let other_log = [
        ("typeinfo", "typeinfo name for COtherDatabase"),
        LIST_SCAN_VALUES[1],
        LIST_SCAN_VALUES[2],
        LIST_SCAN_VALUES[3],
    ];
    for values in [other_insert, other_log] {
        let image = immediate_list("list_scan", &values, "0x1", "0x0");

        assert_eq!(
            lookup(&image, SHIP_LIST).lookup.unwrap_err().reason,
            "reference-list-shape"
        );
    }
}

#[test]
fn an_immediate_list_map_is_equal_when_its_find_is_qualified_over_the_list_items() {
    let find = "NPdxRobinHoodTable::CIterator<CShip> CPdxRobinHoodTable<CPdxUnorderedMap<CString, CShip const*, SPdxHash<CString, void>, std::__1::equal_to<CString>, true>::SMapEntry>::Find<CString>(CString const&) const";
    let values = [
        ("typeinfo", "typeinfo for CShipDatabase"),
        ("insert", SHIP_INSERT),
        ("null", SHIP_NULL),
        ("find", find),
        ("source", "\"parser_util.h\""),
    ];
    let mut image = immediate_list("list_map", &values, "0x1", "0x0");
    image.add(find, shape("map_find"), &[]);
    assert_eq!(
        lookup(&image, SHIP_LIST).lookup.unwrap().key_match,
        Some(KeyMatch::Equal)
    );

    let other_map = find.replace("CString, CShip const*", "CString, COther const*");
    let values = [
        values[0],
        values[1],
        values[2],
        ("find", other_map.as_str()),
        values[4],
    ];
    let image = immediate_list("list_map", &values, "0x1", "0x0");
    assert_eq!(
        lookup(&image, SHIP_LIST).lookup.unwrap_err().reason,
        "reference-list-shape",
        "a map of another element type is not this list's search"
    );
}

/// A deferred ship list whose lambda forwards to an indexed scan, appending with `insert`.
fn deferred_list(insert: &str) -> Image {
    let vtable = LAMBDA_VTABLE.replace(
        "ReadKeyReferenceDeferred<",
        "ReadKeyReferenceDeferredUniform<",
    );
    let forwarded = FORWARDED.replace(
        "ReadKeyReferenceDeferred<",
        "ReadKeyReferenceDeferredUniform<",
    );
    let mut image = Image::default();
    image
        .add(
            SHIP_DEFERRED_LIST,
            shape("deferred_list"),
            &[("lambda", &vtable), ("insert", insert), ("null", SHIP_NULL)],
        )
        .add(
            &lambda_operator(&vtable).unwrap(),
            shape("lambda_forward"),
            &[("forwarded", &forwarded)],
        )
        .add(
            &forwarded,
            shape("forwarded_list_scan"),
            &[
                ("database", "TGameDatabase<CShipDatabase>::_pInstance"),
                ("null", SHIP_NULL),
            ],
        );

    image
}

#[test]
fn a_deferred_list_replaces_each_appended_null_object_with_the_first_equal_item() {
    let insert = "CShip const*& CPdxArray<CShip const*, int>::InsertAtEmplace<CShip const*>(int, CShip const*&&)";
    let fact = lookup(&deferred_list(insert), SHIP_DEFERRED_LIST);

    assert_eq!(
        fact.lookup,
        Ok(Lookup {
            stage: Stage::Deferred,
            key_match: Some(KeyMatch::FirstEqual),
            empty_key_looked_up: true,
            on_missing: Missing::NullObject,
        })
    );
    assert_eq!(
        lookup(&deferred_list(OTHER_INSERT), SHIP_DEFERRED_LIST)
            .lookup
            .unwrap_err()
            .reason,
        "reference-reader-shape",
        "an append of another element type gives the resolver no established destination"
    );
}

#[test]
fn an_immediate_reader_may_scan_keys_behind_a_base_or_call_its_own_getter() {
    let clone = format!("{SHIP_IMMEDIATE} [clone .cold.1]");
    let mut image = Image::default();
    image.add(
        SHIP_IMMEDIATE,
        shape("immediate_scan_base"),
        &[
            ("null", SHIP_NULL),
            ("source", "\"parser_util.h\""),
            ("cold", &clone),
        ],
    );
    assert_eq!(
        lookup(&image, SHIP_IMMEDIATE).lookup.unwrap().key_match,
        Some(KeyMatch::FirstEqual)
    );

    let getter = |folder: &str| {
        let mut image = Image::default();
        image
            .add(
                SHIP_IMMEDIATE,
                shape("immediate_getter"),
                &[
                    ("getter", GETTER),
                    ("folder", folder),
                    ("source", "\"parser_util.h\""),
                    ("cold", &clone),
                ],
            )
            .add(
                GETTER,
                shape("getter_scan"),
                &[
                    ("items", "0x10"),
                    ("count", "0x1c"),
                    ("key", "0x10"),
                    ("length", "0x18"),
                    ("flag", "0x27"),
                    ("null", SHIP_NULL),
                ],
            );
        lookup(&image, SHIP_IMMEDIATE).lookup
    };
    assert_eq!(
        getter("CShipDatabase::GetFolderPath() const")
            .unwrap()
            .key_match,
        Some(KeyMatch::FirstEqual)
    );
    assert_eq!(
        getter("COtherDatabase::GetFolderPath() const")
            .unwrap_err()
            .reason,
        "reference-reader-shape"
    );
}

#[test]
fn a_lambda_getter_may_return_any_validity_slot_and_search_a_hash_table() {
    let find = "CHashTable<CString, CShip, ClassicHashKeyTraits<CString, CShip> >::Find(CString const&) const";
    let mut image = Image::default();
    image
        .add(SHIP, shape("deferred"), &[("lambda", LAMBDA_VTABLE)])
        .add(
            &operator(),
            shape("lambda_getter"),
            &[
                ("database", "CShipDatabase::_pInstance"),
                ("getter", GETTER),
                ("validity", "0x40"),
            ],
        )
        .add(
            GETTER,
            shape("null_getter"),
            &[("table", "0x10"), ("find", find), ("null", SHIP_NULL)],
        )
        .add(
            find,
            shape("hash_find"),
            &[("key", "0x18"), ("length", "0x20"), ("flag", "0x2f")],
        );

    assert_eq!(
        lookup(&image, SHIP).lookup.unwrap().key_match,
        Some(KeyMatch::Equal)
    );
}

#[test]
fn assembled_code_canonicalizes_to_the_lambda_getter_shape() {
    const INSTANCE: u64 = 0x5010;
    const CALLEE: u64 = 0x3000;
    let mut code = Arm64::at(0x1000);
    arm64!(code;
        stp x19, x20, [sp, #-32]!;
        stp x29, x30, [sp, #16];
        add x29, sp, #16;
        mov x20, x0
    );
    code.address(8, INSTANCE);
    arm64!(code; ldr x0, [x8]);
    code.call(CALLEE);
    arm64!(code;
        ldr x8, [x20, #8];
        str x0, [x8];
        ldr x8, [x0];
        ldr x1, [x8, #0x98];
        ldp x29, x30, [sp, #16];
        ldp x19, x20, [sp], #32;
        br x1
    );
    let rows = decode_arm64(&code.bytes(), 0x1000).unwrap();
    let names = BTreeMap::from([
        (INSTANCE, "CShipDatabase::_pInstance".to_owned()),
        (CALLEE, GETTER.to_owned()),
    ]);
    let bindings = LAMBDA_GETTER.matches(&canonical(&rows, &names)).unwrap();

    assert_eq!(bindings["database"], "CShipDatabase::_pInstance");
    assert_eq!(bindings["getter"], GETTER);
}

const TRIGGER_SEARCH: &str = "CTriggerDatabase::CreateTriggerOrScriptedPlaceholder(int, CString const&, CString const&) const";

/// The trigger lookup's facts when its token overload forwards to `search`, built from `body`.
fn trigger_lookup(search: &str, body: &str) -> Result<Lookup, Unresolved> {
    let mut image = Image::default();
    image
        .add(
            crate::engine::analysis::readers::TRIGGER_LOOKUP,
            shape("trigger_token"),
            &[("search", search)],
        )
        .add(TRIGGER_SEARCH, body, &[]);

    analyze(&image.input(&[], &[])).trigger_lookup
}

#[test]
fn the_trigger_lookup_runs_while_reading_and_makes_a_placeholder_on_a_miss() {
    assert_eq!(
        trigger_lookup(TRIGGER_SEARCH, shape("trigger_map")),
        Ok(Lookup {
            stage: Stage::WhileReading,
            key_match: None,
            empty_key_looked_up: true,
            on_missing: Missing::ScriptedTriggerPlaceholder,
        })
    );
}

#[test]
fn a_changed_trigger_search_is_not_a_trigger_lookup() {
    let other_miss = shape("trigger_map").replace(
        "CScriptedTrigger::CScriptedTrigger(CString const&)",
        "CTrigger::CTrigger()",
    );
    let other_branch = shape("trigger_map").replacen("b.gt @+7", "b.lt @+7", 1);
    for body in [other_miss, other_branch] {
        assert_ne!(body, shape("trigger_map"));

        assert_eq!(
            trigger_lookup(TRIGGER_SEARCH, &body).unwrap_err().reason,
            "trigger-search-shape"
        );
    }

    let other_search = "CTriggerDatabase::GetTrigger(int) const";
    assert_eq!(
        trigger_lookup(other_search, shape("trigger_map"))
            .unwrap_err()
            .reason,
        "trigger-search-body"
    );
}
