//! Owner-initialization lookups and the SDK-482 mutation controls that apply to them.
//!
//! Each control changes one line of an authored initializer or getter and asserts that the text
//! changed. Controls 21 to 25 (reader provenance) are in `tests_fields.rs`, the second half of
//! control 17 and control 26 in the session's grammar normalization, and control 27 (an
//! unavailable target) is `installation_errors_are_precise_without_a_game` in
//! `tests/installation.rs`.
use super::super::tests::{Image, shape};
use super::*;
use crate::engine::analysis::assembler::{Arm64, arm64};
use crate::engine::analysis::decode::decode_arm64;
use crate::engine::analysis::directories::Directory;
use crate::engine::analysis::references::analyze;

const SHIP: &str = "CCreateShipEffect::PostInit()";
const DISTRICT: &str = "CAddDistrictEffect::PostInit()";
const PLANET: &str = "CChangePlanetClassEffect::PostInit()";
const SHIP_FIND: &str = "NPdxRobinHoodTable::CIterator<CShipSize> CPdxRobinHoodTable<CPdxUnorderedMap<CString, CShipSize const*, SPdxHash<CString, void>, std::__1::equal_to<CString>, true>::SMapEntry>::Find<CString>(CString const&) const";
const PLANET_GETTER: &str = "CPlanetClassDatabase::GetPlanetClass(CString const&) const";
const PLANET_FIND: &str = "CHashTable<CString, CPlanetClass, ClassicHashKeyTraits<CString, CPlanetClass> >::Find(CString const&) const";

const SHIP_VALUES: &[(&str, &str)] = &[
    ("database", "TGameDatabase<CShipSizeDatabase>::_pInstance"),
    ("find", SHIP_FIND),
    ("null", "TPdxNullObject<CShipSize>::_pInstance"),
    ("input", "0x600"),
    ("input_length", "0x608"),
    ("input_flag", "0x617"),
    ("output", "0x120"),
];

const DISTRICT_VALUES: &[(&str, &str)] = &[
    (
        "database",
        "TGameDatabase<CDistrictTypeDatabase>::_pInstance",
    ),
    ("null", "TPdxNullObject<CDistrictType>::_pInstance"),
    ("input", "0xa8"),
    ("input_length", "0xb0"),
    ("input_flag", "0xbf"),
    ("key", "0x10"),
    ("length", "0x18"),
    ("flag", "0x27"),
    ("output", "0xd0"),
];

const PLANET_VALUES: &[(&str, &str)] = &[
    ("database", "CPlanetClassDatabase::_pInstance"),
    ("getter", PLANET_GETTER),
    ("input", "0x238"),
    ("output", "0x260"),
];

const PLANET_GETTER_VALUES: &[(&str, &str)] = &[
    ("table", "0x10"),
    ("find", PLANET_FIND),
    ("null", "TPdxNullObject<CPlanetClass>::_pInstance"),
];

const PLANET_FIND_VALUES: &[(&str, &str)] =
    &[("key", "0x18"), ("length", "0x20"), ("flag", "0x2f")];

/// `shape` with its first `from` replaced by `to`; the mutation must change the text.
fn mutated(shape: &str, from: &str, to: &str) -> String {
    let changed = shape.replacen(from, to, 1);
    assert_ne!(changed, shape, "the mutation of {from:?} changes the text");

    changed
}

fn ship(initializer: &str) -> Image {
    let mut image = Image::default();
    image
        .add(SHIP, initializer, SHIP_VALUES)
        .add(SHIP_FIND, shape("map_find"), &[]);

    image
}

fn district(initializer: &str, values: &[(&str, &str)]) -> Image {
    let mut image = Image::default();
    image.add(DISTRICT, initializer, values);

    image
}

fn planet(initializer: &str, getter: &str, getter_values: &[(&str, &str)]) -> Image {
    let mut image = Image::default();
    image
        .add(PLANET, initializer, PLANET_VALUES)
        .add(PLANET_GETTER, getter, getter_values)
        .add(PLANET_FIND, shape("hash_find"), PLANET_FIND_VALUES);

    image
}

fn initialization(image: &Image, name: &str) -> Initialization {
    let mut input = image.input(
        &[],
        &[
            (
                "CShipSizeDatabase",
                Directory::Named("common/ship_sizes".into()),
            ),
            (
                "CDistrictTypeDatabase",
                Directory::Named("common/districts".into()),
            ),
            ("CPlanetClassDatabase", Directory::Missing),
        ],
    );
    input.initializers.insert(name.to_owned());

    analyze(&input).initializers[name].clone()
}

fn found(initialization: Initialization) -> InitializationLookup {
    match initialization {
        Initialization::Lookup(lookup) => lookup,
        other => panic!("no lookup: {other:?}"),
    }
}

fn reason(initialization: Initialization) -> &'static str {
    match initialization {
        Initialization::Unresolved(stop) => stop.reason,
        other => panic!("not unresolved: {other:?}"),
    }
}

fn ship_baseline() -> Initialization {
    initialization(&ship(shape("initializer_map_nonempty")), SHIP)
}

fn district_baseline() -> Initialization {
    initialization(
        &district(shape("initializer_scan"), DISTRICT_VALUES),
        DISTRICT,
    )
}

fn planet_image(initializer: &str) -> Image {
    planet(initializer, shape("null_getter"), PLANET_GETTER_VALUES)
}

#[test]
fn control_01_ship_map_skips_an_empty_key_and_selects_the_null_object() {
    assert_eq!(
        found(ship_baseline()),
        InitializationLookup {
            database: "CShipSizeDatabase".into(),
            directory: Some("common/ship_sizes".into()),
            key_offset: 0x600,
            item_offset: 0x120,
            lookup: Lookup {
                stage: Stage::OwnerInitialization,
                key_match: Some(KeyMatch::Equal),
                empty_key_looked_up: Some(false),
                missing_yields_null: Some(true),
            },
        }
    );
}

#[test]
fn control_02_district_scan_selects_the_first_equal_item() {
    assert_eq!(
        found(district_baseline()),
        InitializationLookup {
            database: "CDistrictTypeDatabase".into(),
            directory: Some("common/districts".into()),
            key_offset: 0xa8,
            item_offset: 0xd0,
            lookup: Lookup {
                stage: Stage::OwnerInitialization,
                key_match: Some(KeyMatch::FirstEqual),
                empty_key_looked_up: Some(true),
                missing_yields_null: Some(true),
            },
        }
    );
}

#[test]
fn control_03_planet_getter_is_an_equal_hash_search_with_a_null_substitute() {
    let lookup = found(initialization(
        &planet_image(shape("initializer_getter")),
        PLANET,
    ));

    assert_eq!(lookup.database, "CPlanetClassDatabase");
    assert_eq!(lookup.directory, None, "ownership is a separate join");
    assert_eq!((lookup.key_offset, lookup.item_offset), (0x238, 0x260));
    assert_eq!(
        lookup.lookup,
        Lookup {
            stage: Stage::OwnerInitialization,
            key_match: Some(KeyMatch::Equal),
            empty_key_looked_up: Some(true),
            missing_yields_null: Some(true),
        }
    );
}

#[test]
fn control_04_ship_with_a_wrong_incoming_owner_is_no_lookup() {
    let body = mutated(
        shape("initializer_map_nonempty"),
        "mov xr1,x0",
        "mov xr1,x1",
    );

    assert_eq!(
        reason(initialization(&ship(&body), SHIP)),
        "initializer-shape"
    );
}

#[test]
fn control_05_district_with_a_wrong_incoming_owner_is_no_lookup() {
    let body = mutated(shape("initializer_scan"), "mov xr7,x0", "mov xr7,x1");

    assert_eq!(
        reason(initialization(&district(&body, DISTRICT_VALUES), DISTRICT)),
        "initializer-shape"
    );
}

#[test]
fn control_06_planet_with_a_wrong_incoming_owner_is_no_lookup() {
    let body = mutated(shape("initializer_getter"), "mov xr1,x0", "mov xr1,x1");

    assert_eq!(
        reason(initialization(&planet_image(&body), PLANET)),
        "initializer-shape"
    );
}

#[test]
fn control_07_ship_with_a_clobbered_stored_value_is_no_lookup() {
    let body = mutated(
        shape("initializer_map_nonempty"),
        "str xr2,[xr1,#{output}]",
        "mov xr2,#0x0\nstr xr2,[xr1,#{output}]",
    );

    assert_eq!(
        reason(initialization(&ship(&body), SHIP)),
        "initializer-shape"
    );
}

#[test]
fn control_08_district_with_a_clobbered_stored_value_is_no_lookup() {
    let body = mutated(
        shape("initializer_scan"),
        "str xr0,[xr7,#{output}]",
        "mov xr0,#0x0\nstr xr0,[xr7,#{output}]",
    );

    assert_eq!(
        reason(initialization(&district(&body, DISTRICT_VALUES), DISTRICT)),
        "initializer-shape"
    );
}

#[test]
fn control_09_planet_with_a_clobbered_stored_value_is_no_lookup() {
    let body = mutated(
        shape("initializer_getter"),
        "str x0,[xr1,#{output}]",
        "mov x0,#0x0\nstr x0,[xr1,#{output}]",
    );

    assert_eq!(
        reason(initialization(&planet_image(&body), PLANET)),
        "initializer-shape"
    );
}

#[test]
fn control_10_ship_with_an_intervening_owner_clobber_is_no_lookup() {
    let body = mutated(
        shape("initializer_map_nonempty"),
        "str xr2,[xr1,#{output}]",
        "mov xr1,x2\nstr xr2,[xr1,#{output}]",
    );

    assert_eq!(
        reason(initialization(&ship(&body), SHIP)),
        "initializer-shape"
    );
}

#[test]
fn control_11_district_with_an_intervening_owner_clobber_is_no_lookup() {
    let body = mutated(
        shape("initializer_scan"),
        "str xr0,[xr7,#{output}]",
        "mov xr7,x2\nstr xr0,[xr7,#{output}]",
    );

    assert_eq!(
        reason(initialization(&district(&body, DISTRICT_VALUES), DISTRICT)),
        "initializer-shape"
    );
}

#[test]
fn control_12_planet_with_an_intervening_owner_clobber_is_no_lookup() {
    let body = mutated(
        shape("initializer_getter"),
        "str x0,[xr1,#{output}]",
        "mov xr1,x2\nstr x0,[xr1,#{output}]",
    );

    assert_eq!(
        reason(initialization(&planet_image(&body), PLANET)),
        "initializer-shape"
    );
}

#[test]
fn control_13_ship_with_a_retargeted_empty_key_branch_is_no_lookup() {
    let body = mutated(
        shape("initializer_map_nonempty"),
        "cbz xr2,@+15",
        "cbz xr2,@+14",
    );

    assert_eq!(
        reason(initialization(&ship(&body), SHIP)),
        "initializer-shape"
    );
}

#[test]
fn control_14_ship_with_an_inverted_null_selection_is_no_lookup() {
    let body = mutated(
        shape("initializer_map_nonempty"),
        "csel xr2,xr4,xr3,eq",
        "csel xr2,xr4,xr3,ne",
    );

    assert_eq!(
        reason(initialization(&ship(&body), SHIP)),
        "initializer-shape"
    );
}

#[test]
fn control_15_district_with_an_inconsistent_string_layout_is_no_lookup() {
    for (name, value) in [
        ("input_flag", "0xc0"),
        ("input_length", "0xb8"),
        ("flag", "0x28"),
    ] {
        let values: Vec<(&str, &str)> = DISTRICT_VALUES
            .iter()
            .map(|&(key, original)| (key, if key == name { value } else { original }))
            .collect();
        assert_ne!(values, DISTRICT_VALUES);

        assert_eq!(
            reason(initialization(
                &district(shape("initializer_scan"), &values),
                DISTRICT
            )),
            "initializer-string-layout",
            "{name}"
        );
    }
}

#[test]
fn control_16_a_null_object_of_another_type_is_no_lookup() {
    let wrong_null = [("null", "TPdxNullObject<CShipSize>::_pInstance")];
    let getter_values = [
        PLANET_GETTER_VALUES[0],
        PLANET_GETTER_VALUES[1],
        wrong_null[0],
    ];
    let image = planet(
        shape("initializer_getter"),
        shape("null_getter"),
        &getter_values,
    );
    assert_eq!(
        reason(initialization(&image, PLANET)),
        "initializer-null-type"
    );

    let mut image = Image::default();
    let ship_values: Vec<(&str, &str)> = SHIP_VALUES
        .iter()
        .map(|&(key, value)| match key {
            "null" => (key, "TPdxNullObject<CDistrictType>::_pInstance"),
            _ => (key, value),
        })
        .collect();
    image.add(SHIP, shape("initializer_map_nonempty"), &ship_values);
    assert_eq!(
        reason(initialization(&image, SHIP)),
        "initializer-null-type"
    );
}

#[test]
fn control_17_coherent_changed_slots_stay_a_lookup_at_the_new_offsets() {
    let values: Vec<(&str, &str)> = DISTRICT_VALUES
        .iter()
        .map(|&(key, value)| match key {
            "input" => (key, "0x1a8"),
            "input_length" => (key, "0x1b0"),
            "input_flag" => (key, "0x1bf"),
            "output" => (key, "0x1d0"),
            _ => (key, value),
        })
        .collect();
    let lookup = found(initialization(
        &district(shape("initializer_scan"), &values),
        DISTRICT,
    ));

    assert_eq!((lookup.key_offset, lookup.item_offset), (0x1a8, 0x1d0));
    assert_eq!(lookup.lookup, found(district_baseline()).lookup);
}

#[test]
fn control_18_district_with_a_retargeted_mismatch_branch_is_no_lookup() {
    let body = mutated(shape("initializer_scan"), "b.ne @-10", "b.ne @-9");

    assert_eq!(
        reason(initialization(&district(&body, DISTRICT_VALUES), DISTRICT)),
        "initializer-shape"
    );
}

#[test]
fn control_19_district_that_calls_strlen_for_memcmp_is_no_lookup() {
    let body = mutated(
        shape("initializer_scan"),
        "CALL = _memcmp",
        "CALL = _strlen",
    );

    assert_eq!(
        reason(initialization(&district(&body, DISTRICT_VALUES), DISTRICT)),
        "initializer-shape"
    );
}

/// The planet class getter as compiled code at `0x1000`: search the hash table at `this+0x10`
/// with the search at `0x3000`, then select the null object at `0x5010` for a null result, or
/// for a found item when `null_on_equal` is false. The image's own addresses start at `0x10000`.
fn assembled_getter(null_on_equal: bool) -> Vec<crate::engine::analysis::decode::Instruction> {
    const FIND: u64 = 0x3000;
    const NULL: u64 = 0x5010;
    let mut code = Arm64::at(0x1000);
    arm64!(code;
        stp x29, x30, [sp, #-16]!;
        mov x29, sp;
        add x0, x0, #0x10
    );
    code.call(FIND).load(8, NULL);
    arm64!(code;
        ldr x8, [x8];
        cmp x0, #0
    );
    if null_on_equal {
        arm64!(code; csel x0, x8, x0, eq); // a null result selects the null object
    } else {
        arm64!(code; csel x0, x8, x0, ne); // a found item selects the null object
    }
    arm64!(code;
        ldp x29, x30, [sp], #16;
        ret
    );

    decode_arm64(&code.bytes(), 0x1000).unwrap()
}

#[test]
fn control_20_a_changed_getter_body_is_no_lookup() {
    for (null_on_equal, expected) in [(true, None), (false, Some("initializer-getter-shape"))] {
        let mut image = Image::default();
        image
            .add(PLANET, shape("initializer_getter"), PLANET_VALUES)
            .add(PLANET_FIND, shape("hash_find"), PLANET_FIND_VALUES);
        let mut input = image.input(&[], &[]);
        input.initializers.insert(PLANET.into());
        input
            .functions
            .insert(PLANET_GETTER.into(), assembled_getter(null_on_equal));
        input.names.insert(0x3000, PLANET_FIND.into());
        input
            .names
            .insert(0x5010, "TPdxNullObject<CPlanetClass>::_pInstance".into());

        match (&analyze(&input).initializers[PLANET], expected) {
            (Initialization::Lookup(lookup), None) => {
                assert_eq!(lookup.lookup.key_match, Some(KeyMatch::Equal));
            }
            (Initialization::Unresolved(stop), Some(reason)) => assert_eq!(stop.reason, reason),
            (other, _) => panic!("{null_on_equal}: {other:?}"),
        }
    }
}

#[test]
fn an_initializer_that_names_no_database_makes_no_lookup() {
    let mut image = Image::default();
    image.add(SHIP, "ret", &[]);

    assert_eq!(initialization(&image, SHIP), Initialization::NoLookup);
}

#[test]
fn an_unmatched_initializer_that_names_several_databases_is_counted_apart() {
    let mut image = Image::default();
    image.add(
        SHIP,
        "adrp xr0,PAGE\nldr xr0,[xr0,G] = TGameDatabase<CShipSizeDatabase>::_pInstance\nadrp xr1,PAGE\nldr xr1,[xr1,G] = TGameDatabase<CDistrictTypeDatabase>::_pInstance\nret",
        &[],
    );

    assert_eq!(
        reason(initialization(&image, SHIP)),
        "initializer-several-lookups"
    );
}

#[test]
fn a_missing_initializer_body_is_unresolved() {
    assert_eq!(
        reason(initialization(&Image::default(), SHIP)),
        "initializer-body"
    );
}

#[test]
fn a_nonempty_scan_and_a_plain_map_state_their_empty_key() {
    let values: Vec<(&str, &str)> = DISTRICT_VALUES.to_vec();
    let scan = found(initialization(
        &district(shape("initializer_scan_nonempty"), &values),
        DISTRICT,
    ));
    assert_eq!(scan.lookup.empty_key_looked_up, Some(false));
    assert_eq!(scan.lookup.key_match, Some(KeyMatch::FirstEqual));

    let map = found(initialization(&ship(shape("initializer_map")), SHIP));
    assert_eq!(map.lookup.empty_key_looked_up, Some(true));
    assert_eq!(map.lookup.key_match, Some(KeyMatch::Equal));
}

#[test]
fn a_getter_whose_hash_search_is_not_qualified_has_no_key_match() {
    let mut image = planet_image(shape("initializer_getter"));
    let hashless = mutated(shape("hash_find"), "CALL = _PMurHash32", "CALL = _rand");
    image.add(PLANET_FIND, &hashless, PLANET_FIND_VALUES);

    assert_eq!(found(initialization(&image, PLANET)).lookup.key_match, None);
}

#[test]
fn a_getter_of_another_database_is_no_lookup() {
    let mut image = Image::default();
    let values = [
        ("database", "CShipSizeDatabase::_pInstance"),
        ("getter", PLANET_GETTER),
        ("input", "0x238"),
        ("output", "0x260"),
    ];
    image.add(PLANET, shape("initializer_getter"), &values);

    assert_eq!(reason(initialization(&image, PLANET)), "initializer-getter");
}

#[test]
fn a_miss_that_selects_no_typed_null_object_is_no_lookup() {
    let values: Vec<(&str, &str)> = DISTRICT_VALUES
        .iter()
        .map(|&(key, value)| match key {
            "null" => (key, "CDistrictType::s_Default"),
            _ => (key, value),
        })
        .collect();

    assert_eq!(
        reason(initialization(
            &district(shape("initializer_scan"), &values),
            DISTRICT
        )),
        "initializer-null-object"
    );
}
