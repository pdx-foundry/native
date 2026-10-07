//! Authored-input tests of the instance-pointer proof, with negative controls.
use std::collections::BTreeMap;

use super::instance_vtables;
use crate::engine::analysis::callbacks::tests::{Rows, rows};
use crate::engine::analysis::evaluate::ReadOnlyData;

/// The instance pointer at 0x7100 and an initialization flag at 0x7110, each named by a GOT slot.
const POINTER: u64 = 0x7100;
const POINT: u64 = 0x6010;
const OTHER_POINT: u64 = 0x6020;

fn data() -> ReadOnlyData {
    let mut slots = POINTER.to_le_bytes().to_vec();
    slots.extend(0x7110u64.to_le_bytes());
    ReadOnlyData::new(vec![(0x7000, slots)])
}

fn prove(writers: &[Rows<'_>]) -> BTreeMap<u64, u64> {
    let writers = BTreeMap::from([(POINTER, writers.iter().map(|lines| rows(lines)).collect())]);

    instance_vtables(&writers, &data())
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

/// Reads the pointer, then branches on seven unknown values, which make more paths than a run
/// follows.
fn branches_after_reading() -> Vec<(u64, &'static str, &'static str)> {
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
        (0x2028, "ret", ""),
    ]
}

#[test]
fn a_writer_whose_search_stops_at_its_bound_rejects_the_pointer() {
    assert!(prove(&[INITIALIZES, &branches_after_reading()]).is_empty());
}

#[test]
fn a_call_that_receives_the_object_after_its_vtable_rejects_the_pointer() {
    let mut passes_on = INITIALIZES.to_vec();
    passes_on[14] = (0x1038, "mov", "x0,x9"); // the object
    passes_on[15] = (0x103c, "bl", "#0x9900");

    assert!(prove(&[&passes_on]).is_empty());
}
