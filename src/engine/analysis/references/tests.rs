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
/// registers `xrN` become `x(8+N)`, and every named address gets its own location.
#[derive(Default)]
pub(super) struct Image {
    functions: BTreeMap<String, Vec<Instruction>>,
    names: BTreeMap<u64, String>,
    next_function: u64,
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

    /// Add the function `name`, built from `shape` with `values` substituted for placeholders.
    pub(super) fn add(&mut self, name: &str, shape: &str, values: &[(&str, &str)]) -> &mut Self {
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
            let page = self.address_of(next.expect("an adrp completes a named address")) & !0xfff;
            operands = operands.replace("PAGE", &format!("#{page:#x}"));
        } else if let Some(value) = value.filter(|_| operands.contains('G')) {
            let offset = self.address_of(value) & 0xfff;
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
        "map_find" => include_str!("shapes/map_find.shape"),
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
            empty_key_looked_up: Some(true),
            missing_yields_null: Some(true),
        })
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

    assert_eq!(
        lookup(&image, SHIP).lookup.unwrap_err().reason,
        "reference-lambda-shape"
    );
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
    let unqualified = lookup(&image, SHIP).lookup.unwrap();
    assert_eq!(unqualified.key_match, None);

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
    assert_eq!(
        lookup(&image, SHIP_IMMEDIATE).lookup,
        Ok(Lookup {
            stage: Stage::WhileReading,
            key_match: Some(KeyMatch::FirstEqual),
            empty_key_looked_up: Some(true),
            missing_yields_null: Some(true),
        })
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
        lookup(&image, SHIP).lookup
    };

    assert_eq!(
        getter("0x16f").unwrap().key_match,
        Some(KeyMatch::FirstEqual)
    );
    assert!(getter("0x170").is_err());
}

#[test]
fn list_and_index_readers_keep_their_lookup_unresolved() {
    let list = "void NParserUtil::ReadKeyReferenceDeferredUniform<CShipDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CPdxArray<CShipDatabase::ValueType const*, int>&)";
    let fact = lookup(&Image::default(), list);

    assert_eq!(fact.directory.as_deref(), Some("common/ships"));
    assert_eq!(fact.lookup.unwrap_err().reason, "reference-list-form");
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
