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
            (
                "CCitizenshipTypeDatabase",
                Directory::Named("common/species_rights/citizenship_types".into()),
            ),
            ("CEventManager", Directory::Named("events".into())),
            (
                "COnActionDatabase",
                Directory::Named("common/on_actions".into()),
            ),
            (
                "CStrategicResourceDatabase",
                Directory::Named("common/strategic_resources".into()),
            ),
            (
                "CEconomicCategoryDatabase",
                Directory::Named("common/economic_categories".into()),
            ),
        ],
    );
    input.initializers.insert(name.to_owned());

    analyze(&input).initializers[name].clone()
}

fn found(initialization: Initialization) -> InitializationLookup {
    match initialization {
        Initialization::Lookups(lookups) if lookups.len() == 1 => lookups[0].clone(),
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
                empty_key_looked_up: false,
                on_missing: Some(Missing::NullObject),
            },
            unstated: None,
            execution: Some(Execution::Inline {
                null: "TPdxNullObject<CShipSize>::_pInstance".into(),
            }),
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
                empty_key_looked_up: true,
                on_missing: Some(Missing::NullObject),
            },
            unstated: None,
            execution: Some(Execution::Inline {
                null: "TPdxNullObject<CDistrictType>::_pInstance".into(),
            }),
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
            empty_key_looked_up: true,
            on_missing: Some(Missing::NullObject),
        }
    );
    assert_eq!(
        lookup.execution,
        Some(Execution::Getter {
            database: "CPlanetClassDatabase::_pInstance".into(),
            getter: PLANET_GETTER.into(),
            null: "TPdxNullObject<CPlanetClass>::_pInstance".into(),
        })
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
            (Initialization::Lookups(lookups), None) => {
                assert_eq!(lookups[0].lookup.key_match, Some(KeyMatch::Equal));
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
    assert!(!scan.lookup.empty_key_looked_up);
    assert_eq!(scan.lookup.key_match, Some(KeyMatch::FirstEqual));

    let map = found(initialization(&ship(shape("initializer_map")), SHIP));
    assert!(map.lookup.empty_key_looked_up);
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

#[test]
fn initializer_execution_requires_the_whole_inline_body_and_an_unconditional_lookup() {
    let inline = Some(Execution::Inline {
        null: "TPdxNullObject<CDistrictType>::_pInstance".into(),
    });
    let original = found(initialization(
        &district(shape("initializer_scan"), DISTRICT_VALUES),
        DISTRICT,
    ));
    assert_eq!(original.execution, inline);
    assert!(original.lookup.empty_key_looked_up);

    let extra_store = mutated(shape("initializer_scan"), "ret", "str xzr,[x0,#0x78]\nret");
    let changed = district(&extra_store, DISTRICT_VALUES);
    assert_eq!(
        reason(initialization(&changed, DISTRICT)),
        "initializer-shape"
    );

    let conditional = found(initialization(
        &district(shape("initializer_scan_nonempty"), DISTRICT_VALUES),
        DISTRICT,
    ));
    assert_eq!(conditional.execution, inline);
    assert!(!conditional.lookup.empty_key_looked_up);
}

const RIGHTS: &str = "CSetSpeciesRightsEffect<SSetCitizenshipType>::PostInit()";
const RIGHTS_TRIGGER: &str = "CHasSpeciesRightsTrigger<SHasCitizenshipType>::PostInit()";

const RIGHTS_SCAN_VALUES: &[(&str, &str)] = &[
    (
        "database",
        "TGameDatabase<CCitizenshipTypeDatabase>::_pInstance",
    ),
    ("null", "TPdxNullObject<CCitizenshipType>::_pInstance"),
    ("key", "0x10"),
    ("length", "0x18"),
    ("flag", "0x27"),
];

/// A species-rights initializer whose scanned item's validity check returns `item_valid`; the null
/// object's returns 0.
fn rights(name: &str, initializer: &str, owner: &[(&str, &str)], item_valid: &str) -> Image {
    let values: Vec<_> = RIGHTS_SCAN_VALUES.iter().chain(owner).copied().collect();
    let mut image = Image::default();
    image
        .add(name, initializer, &values)
        .validity("CCitizenshipType", item_valid)
        .validity("TPdxNullObject<CCitizenshipType>", "0x0");

    image
}

fn set_rights(item_valid: &str) -> Initialization {
    let owner = [
        ("input", "0x240"),
        ("input_length", "0x248"),
        ("input_flag", "0x257"),
        ("output", "0xa8"),
    ];

    initialization(
        &rights(RIGHTS, shape("initializer_scan_valid"), &owner, item_valid),
        RIGHTS,
    )
}

#[test]
fn a_scan_that_stores_only_a_valid_item_leaves_the_miss_unstated() {
    let lookup = found(set_rights("0x1"));

    assert_eq!(
        lookup.directory.as_deref(),
        Some("common/species_rights/citizenship_types")
    );
    assert_eq!((lookup.key_offset, lookup.item_offset), (0x240, 0xa8));
    assert_eq!(
        lookup.lookup,
        Lookup {
            stage: Stage::OwnerInitialization,
            key_match: Some(KeyMatch::FirstEqual),
            empty_key_looked_up: true,
            on_missing: None,
        }
    );
    assert_eq!(lookup.unstated, Some(NULL_POINTER));
    assert_eq!(
        lookup.execution, None,
        "the initializer stores no typed null object"
    );
}

#[test]
fn a_valid_scan_needs_an_item_check_that_always_passes() {
    assert_eq!(reason(set_rights("0x0")), "initializer-validity");
}

#[test]
fn the_species_rights_trigger_scan_also_states_its_keyword_flag() {
    let owner = [
        ("input", "0x200"),
        ("input_length", "0x208"),
        ("input_flag", "0x217"),
        ("output", "0x68"),
        ("flag_output", "0x228"),
    ];
    let image = rights(
        RIGHTS_TRIGGER,
        shape("initializer_scan_valid_flag"),
        &owner,
        "0x1",
    );
    let lookup = found(initialization(&image, RIGHTS_TRIGGER));

    assert_eq!((lookup.key_offset, lookup.item_offset), (0x200, 0x68));
    assert_eq!(lookup.lookup.on_missing, None);
    assert_eq!(lookup.unstated, Some(KEYWORD_FLAG));
}

const EVENT: &str = "CFireEventEffect::PostInit()";
const EVENT_GETTER_NAME: &str = "CEventManager::GetEvent(CString const&) const";
const ON_ACTION: &str = "CFireOnActionEffect::PostInit()";
const ON_ACTION_GETTER: &str = "COnActionDatabase::GetOnActionList(CString const&) const";
const ON_ACTION_FIND: &str = "CHashTable<CString, COnActionList const, ClassicHashKeyTraits<CString, COnActionList const> >::Find(CString const&) const";

/// An initializer that calls `getter` on `database`, whose body is `body`.
fn getter_call(
    name: &str,
    database: &str,
    getter: &str,
    body: &str,
    values: &[(&str, &str)],
) -> Initialization {
    let mut image = Image::default();
    image
        .add(
            name,
            shape("initializer_getter"),
            &[
                ("database", database),
                ("getter", getter),
                ("input", "0xa8"),
                ("output", "0xd0"),
            ],
        )
        .add(getter, body, values);

    initialization(&image, name)
}

#[test]
fn the_event_getter_gives_the_target_but_not_its_id_match_or_miss() {
    let lookup = found(getter_call(
        EVENT,
        "CEventManager::_pInstance",
        EVENT_GETTER_NAME,
        shape("event_getter"),
        &[],
    ));

    assert_eq!(lookup.directory.as_deref(), Some("events"));
    assert_eq!(lookup.lookup.key_match, None);
    assert_eq!(lookup.lookup.on_missing, None);
    assert_eq!(lookup.unstated, Some(EVENT_ID));
    assert_eq!(lookup.execution, None);
}

#[test]
fn a_getter_that_returns_its_hash_search_matches_equal_keys_and_misses_with_null() {
    let mut image = Image::default();
    image
        .add(
            ON_ACTION,
            shape("initializer_getter"),
            &[
                ("database", "COnActionDatabase::_pInstance"),
                ("getter", ON_ACTION_GETTER),
                ("input", "0xa8"),
                ("output", "0xd0"),
            ],
        )
        .add(
            ON_ACTION_GETTER,
            shape("hash_getter"),
            &[("find", ON_ACTION_FIND)],
        )
        .add(ON_ACTION_FIND, shape("hash_find"), PLANET_FIND_VALUES);
    let lookup = found(initialization(&image, ON_ACTION));

    assert_eq!(lookup.directory.as_deref(), Some("common/on_actions"));
    assert_eq!(lookup.lookup.key_match, Some(KeyMatch::Equal));
    assert_eq!(lookup.lookup.on_missing, None);
    assert_eq!(lookup.unstated, Some(NULL_POINTER));
}

#[test]
fn a_getter_whose_returned_hash_search_is_not_qualified_states_both_reasons() {
    let lookup = found(getter_call(
        ON_ACTION,
        "COnActionDatabase::_pInstance",
        ON_ACTION_GETTER,
        shape("hash_getter"),
        &[("find", ON_ACTION_FIND)],
    ));

    assert_eq!(lookup.lookup.key_match, None);
    assert_eq!(lookup.unstated, Some(UNQUALIFIED_NULL_POINTER));
}

#[test]
fn a_changed_event_getter_is_no_lookup() {
    let changed = mutated(shape("event_getter"), "mov wr2,#0x86a0", "mov wr2,#0x86a1");

    assert_eq!(
        reason(getter_call(
            EVENT,
            "CEventManager::_pInstance",
            EVENT_GETTER_NAME,
            &changed,
            &[],
        )),
        "initializer-getter-shape"
    );
}

const COMPARE: &str = "CResourceCompareTrigger<SResourceIncomeCompare>::PostInit()";
const RESOURCE_GETTER: &str = "CStrategicResourceDatabase::GetResource(CString const&) const";

#[test]
fn a_getter_then_a_nonempty_scan_gives_one_lookup_for_each_key() {
    let mut image = Image::default();
    image
        .add(
            COMPARE,
            shape("initializer_getter_scan_nonempty"),
            &[
                ("call_database", "CStrategicResourceDatabase::_pInstance"),
                ("call_getter", RESOURCE_GETTER),
                ("call_input", "0x280"),
                ("call_output", "0x270"),
                (
                    "database",
                    "TGameDatabase<CEconomicCategoryDatabase>::_pInstance",
                ),
                ("null", "TPdxNullObject<CEconomicCategory>::_pInstance"),
                ("input", "0x2a8"),
                ("input_length", "0x2b0"),
                ("input_flag", "0x2bf"),
                ("key", "0x10"),
                ("length", "0x18"),
                ("flag", "0x27"),
                ("output", "0x278"),
            ],
        )
        .add(
            RESOURCE_GETTER,
            shape("getter_scan"),
            &[
                ("items", "0x3d0"),
                ("count", "0x3dc"),
                ("key", "0x158"),
                ("length", "0x160"),
                ("flag", "0x16f"),
                ("null", "TPdxNullObject<CStrategicResource>::_pInstance"),
            ],
        );
    let Initialization::Lookups(lookups) = initialization(&image, COMPARE) else {
        panic!("no lookups");
    };
    let summary: Vec<_> = lookups
        .iter()
        .map(|lookup| {
            (
                lookup.directory.as_deref(),
                lookup.key_offset,
                lookup.item_offset,
                lookup.lookup.empty_key_looked_up,
                lookup.lookup.on_missing,
                lookup.execution.is_some(),
            )
        })
        .collect();

    assert_eq!(
        summary,
        [
            (
                Some("common/strategic_resources"),
                0x280,
                0x270,
                true,
                Some(Missing::NullObject),
                false,
            ),
            (
                Some("common/economic_categories"),
                0x2a8,
                0x278,
                false,
                Some(Missing::NullObject),
                false,
            ),
        ]
    );
}
