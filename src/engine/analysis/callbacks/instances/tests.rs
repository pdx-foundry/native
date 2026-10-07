//! Authored-input tests of the instance-pointer proof, with negative controls.
use std::collections::{BTreeMap, BTreeSet};

use super::{Loader, instance_vtables, virtual_call_slots, written_slots};
use crate::engine::analysis::callbacks::tests::{Rows, rows};
use crate::engine::analysis::evaluate::ReadOnlyData;

/// The instance pointer at 0x7100 and an initialization flag at 0x7110, named by the GOT slots at
/// 0x7000 and 0x7008.
const SLOT: u64 = 0x7000;
const POINTER: u64 = 0x7100;
const POINT: u64 = 0x6010;
const OTHER_POINT: u64 = 0x6020;

fn data() -> ReadOnlyData {
    let mut slots = POINTER.to_le_bytes().to_vec();
    slots.extend(0x7110u64.to_le_bytes());
    ReadOnlyData::new(vec![(0x7000, slots)])
}

fn pointers() -> BTreeMap<u64, u64> {
    BTreeMap::from([(SLOT, POINTER), (SLOT + 8, 0x7110)])
}

/// The proof over loaders of the pointer's slot with these rows.
fn prove(loaders: &[Rows<'_>]) -> BTreeMap<u64, u64> {
    let loaders: Vec<Loader> = loaders
        .iter()
        .map(|lines| Loader {
            rows: Some(rows(lines)),
            slots: BTreeSet::from([SLOT]),
        })
        .collect();

    instance_vtables(&loaders, &pointers(), &data())
}

/// Builds the object once, as a null object's initializer does, then sets the flag.
const INITIALIZES: Rows<'static> = &[
    (0x1000, "adrp", "x19,#0x7000"),
    (0x1004, "ldr", "x19,[x19]"),
    (0x1008, "ldr", "x8,[x19]"),
    (0x100c, "adrp", "x20,#0x7000"),
    (0x1010, "ldr", "x20,[x20,#0x8]"),
    (0x1014, "ldrb", "w9,[x20]"), // the flag, unknown
    (0x1018, "cbnz", "w9,#0x1040"),
    (0x101c, "mov", "x0,x8"),
    (0x1020, "bl", "#0x9900"), // a base constructor that the run does not follow
    (0x1024, "adrp", "x8,#0x6000"),
    (0x1028, "add", "x8,x8,#0x10"),
    (0x102c, "ldr", "x9,[x19]"),
    (0x1030, "str", "x8,[x9]"),
    (0x1034, "mov", "w10,#0x1"),
    (0x1038, "strb", "w10,[x20]"),
    (0x103c, "nop", ""),
    (0x1040, "ret", ""),
];

/// Loads the object, runs `value_rows` from 0x200c to put a vtable in `x8`, and stores it.
fn stores(value_rows: &[(&'static str, &'static str)]) -> Vec<(u64, &'static str, &'static str)> {
    let mut lines = vec![
        (0x2000, "adrp", "x19,#0x7000"),
        (0x2004, "ldr", "x19,[x19]"),
        (0x2008, "ldr", "x21,[x19]"),
    ];
    lines.extend(
        value_rows
            .iter()
            .map(|(operation, operands)| (0, *operation, *operands)),
    );
    lines.extend([(0, "str", "x8,[x21]"), (0, "ret", "")]);
    for (index, line) in lines.iter_mut().enumerate() {
        line.0 = 0x2000 + 4 * index as u64;
    }
    lines
}

#[test]
fn an_initializer_that_stores_one_point_proves_the_vtable() {
    assert_eq!(prove(&[INITIALIZES]), [(POINTER, POINT)].into());
}

#[test]
fn a_reader_alone_proves_nothing() {
    let reads: Rows<'_> = &[
        (0x2000, "adrp", "x19,#0x7000"),
        (0x2004, "ldr", "x19,[x19]"),
        (0x2008, "ldr", "x8,[x19]"),
        (0x200c, "ldr", "x9,[x8]"),
        (0x2010, "ret", ""),
    ];

    assert!(prove(&[reads]).is_empty());
    assert_eq!(prove(&[INITIALIZES, reads]), [(POINTER, POINT)].into());
}

#[test]
fn a_second_writer_with_another_point_rejects_the_pointer() {
    let other = stores(&[("adrp", "x8,#0x6000"), ("add", "x8,x8,#0x20")]);

    assert_eq!(prove(&[&other]), [(POINTER, OTHER_POINT)].into());
    assert!(prove(&[INITIALIZES, &other]).is_empty());
}

#[test]
fn a_writer_that_stores_an_unknown_vtable_rejects_the_pointer() {
    let unknown = stores(&[("bl", "#0x9900"), ("mov", "x8,x0")]);

    assert!(prove(&[INITIALIZES, &unknown]).is_empty());
}

#[test]
fn a_writer_that_replaces_the_object_rejects_the_pointer() {
    let replaces: Rows<'_> = &[
        (0x2000, "adrp", "x19,#0x7000"),
        (0x2004, "ldr", "x19,[x19]"),
        (0x2008, "bl", "#0x9900"), // allocates the new object
        (0x200c, "str", "x0,[x19]"),
        (0x2010, "ret", ""),
    ];

    assert!(prove(&[INITIALIZES, replaces]).is_empty());
}

#[test]
fn a_replacement_conditional_on_the_vtable_rejects_the_pointer() {
    let swaps = stores(&[
        ("ldr", "x10,[x21]"),
        ("adrp", "x11,#0x6000"),
        ("add", "x11,x11,#0x10"),
        ("cmp", "x10,x11"),
        ("b.ne", "#0x202c"), // to the return: only an object with the first vtable changes
        ("adrp", "x8,#0x6000"),
        ("add", "x8,x8,#0x20"),
    ]);

    assert!(prove(&[INITIALIZES, &swaps]).is_empty());
}

#[test]
fn a_store_through_an_unknown_address_keeps_the_proof() {
    let mut lines = INITIALIZES.to_vec();
    lines[15] = (0x103c, "str", "x0,[x1]");

    assert_eq!(prove(&[&lines]), [(POINTER, POINT)].into());
}

#[test]
fn a_destructor_that_clears_the_pointer_keeps_the_proof() {
    let destroys: Rows<'_> = &[
        (0x2000, "adrp", "x8,#0x6000"),
        (0x2004, "add", "x8,x8,#0x10"),
        (0x2008, "str", "x8,[x0]"), // its own vtable, through its receiver
        (0x200c, "adrp", "x9,#0x7000"),
        (0x2010, "ldr", "x9,[x9]"),
        (0x2014, "str", "xzr,[x9]"),
        (0x2018, "ret", ""),
    ];

    assert_eq!(prove(&[INITIALIZES, destroys]), [(POINTER, POINT)].into());
}

/// Loads the object, branches on seven unknown values, which make more paths than a run follows,
/// and then runs `last`.
fn branches_then(last: (&'static str, &'static str)) -> Vec<(u64, &'static str, &'static str)> {
    vec![
        (0x2000, "adrp", "x19,#0x7000"),
        (0x2004, "ldr", "x19,[x19]"),
        (0x2008, "ldr", "x8,[x19]"),
        (0x200c, "cbz", "x1,#0x2010"),
        (0x2010, "cbz", "x2,#0x2014"),
        (0x2014, "cbz", "x3,#0x2018"),
        (0x2018, "cbz", "x4,#0x201c"),
        (0x201c, "cbz", "x5,#0x2020"),
        (0x2020, "cbz", "x6,#0x2024"),
        (0x2024, "cbz", "x7,#0x2028"),
        (0x2028, last.0, last.1),
        (0x202c, "ret", ""),
    ]
}

#[test]
fn a_writer_whose_search_stops_at_its_bound_rejects_the_pointer() {
    let writes_a_field = branches_then(("str", "xzr,[x8,#0x8]"));

    assert!(prove(&[INITIALIZES, &writes_a_field]).is_empty());
}

#[test]
fn a_loader_that_only_reads_does_not_run() {
    let reads = branches_then(("ldr", "x9,[x8,#0x8]"));

    assert_eq!(prove(&[INITIALIZES, &reads]), [(POINTER, POINT)].into());
}

#[test]
fn a_loader_that_does_not_decode_rejects_the_pointer() {
    let loaders = [
        Loader {
            rows: Some(rows(INITIALIZES)),
            slots: BTreeSet::from([SLOT]),
        },
        Loader {
            rows: None,
            slots: BTreeSet::from([SLOT]),
        },
    ];

    assert!(instance_vtables(&loaders, &pointers(), &data()).is_empty());
}

#[test]
fn a_call_that_receives_the_object_after_its_vtable_rejects_the_pointer() {
    let mut passes_on = INITIALIZES.to_vec();
    passes_on[14] = (0x1038, "mov", "x0,x9"); // the object
    passes_on[15] = (0x103c, "bl", "#0x9900");

    assert!(prove(&[&passes_on]).is_empty());
}

/// `CTraditionType::GetUnlocksAgenda` on M452: the null swap's virtual call after the loop, with
/// the loop's `mov x20,x22` between the receiver's load and the call.
#[test]
fn a_virtual_call_through_an_instance_pointer_names_its_slot() {
    let unlocks = rows(&[
        (0x1000, "adrp", "x8,#0x7000"),
        (0x1004, "ldr", "x8,[x8]"),
        (0x1008, "ldr", "x20,[x8]"),
        (0x100c, "ldr", "w8,[x0,#0x5dc]"),
        (0x1010, "b.lt", "#0x1020"),
        (0x1014, "mov", "x20,x22"), // a swap that the loop selected
        (0x1018, "b", "#0x1010"),
        (0x101c, "nop", ""),
        (0x1020, "ldr", "x8,[x20]"),
        (0x1024, "ldr", "x8,[x8,#0x40]"),
        (0x1028, "mov", "x0,x20"),
        (0x102c, "blr", "x8"),
        (0x1030, "ret", ""),
    ]);

    assert_eq!(
        virtual_call_slots(&unlocks, &pointers()),
        BTreeSet::from([SLOT])
    );
}

#[test]
fn a_virtual_call_through_an_argument_names_no_slot() {
    let call = rows(&[
        (0x1000, "ldr", "x8,[x0]"),
        (0x1004, "ldr", "x8,[x8,#0x40]"),
        (0x1008, "blr", "x8"),
        (0x100c, "ret", ""),
    ]);

    assert!(virtual_call_slots(&call, &pointers()).is_empty());
}

/// Loads the instance pointer's object into `x20`, runs `middle`, and stores `x9` at the address
/// in `x20`.
fn stores_through_x20(
    middle: &[(&'static str, &'static str)],
) -> Vec<(u64, &'static str, &'static str)> {
    let mut lines = vec![
        (0, "adrp", "x8,#0x7000"),
        (0, "ldr", "x8,[x8]"),
        (0, "ldr", "x20,[x8]"),
    ];
    lines.extend(
        middle
            .iter()
            .map(|(operation, operands)| (0, *operation, *operands)),
    );
    lines.extend([(0, "str", "x9,[x20]"), (0, "ret", "")]);
    for (index, line) in lines.iter_mut().enumerate() {
        line.0 = 0x1000 + 4 * index as u64;
    }
    lines
}

#[test]
fn a_register_written_on_every_path_no_longer_holds_the_object() {
    let reused = stores_through_x20(&[("ldr", "w20,[x22,#0x5dc]")]);

    assert!(written_slots(&rows(&reused), &pointers()).is_empty());
}

#[test]
fn a_register_written_on_one_path_may_still_hold_the_object() {
    let around = stores_through_x20(&[
        ("cbz", "x1,#0x1014"),
        ("mov", "x20,x22"), // only when x1 is not zero
    ]);

    assert_eq!(
        written_slots(&rows(&around), &pointers()),
        BTreeSet::from([SLOT])
    );
}

#[test]
fn a_store_to_the_pointer_and_a_call_that_receives_it_write_through_the_slot() {
    let clears: Rows<'_> = &[
        (0x1000, "adrp", "x8,#0x7000"),
        (0x1004, "ldr", "x8,[x8]"),
        (0x1008, "str", "xzr,[x8]"),
        (0x100c, "ret", ""),
    ];
    let passes: Rows<'_> = &[
        (0x1000, "adrp", "x0,#0x7000"),
        (0x1004, "ldr", "x0,[x0]"),
        (0x1008, "bl", "#0x9900"),
        (0x100c, "ret", ""),
    ];

    assert_eq!(
        written_slots(&rows(clears), &pointers()),
        BTreeSet::from([SLOT])
    );
    assert_eq!(
        written_slots(&rows(passes), &pointers()),
        BTreeSet::from([SLOT])
    );
}
