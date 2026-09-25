use super::*;
use crate::binding::binary::discovery;
use crate::engine::analysis::analysis_support as support;

const READ: u64 = 0x1_0000_1000;
const HELPER: u64 = 0x1_0000_1020;
const STRING: u64 = 0x1_0000_2008;
const SLOT: u64 = 0x1_0000_4000;

fn notes_at(listing: &Listing, address: u64) -> &[Note] {
    &listing
        .rows
        .iter()
        .find(|row| row.address == address)
        .expect("a row at the address")
        .notes
}

fn has_pointer(listing: &Listing) -> bool {
    listing
        .rows
        .iter()
        .flat_map(|row| &row.notes)
        .any(|note| matches!(note, Note::Pointer(_)))
}

#[test]
fn an_uncatalogued_image_is_identified_and_disassembled_without_a_target_record() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("stellaris");
    std::fs::write(&path, support::macho_image(6)).unwrap();

    let bytes = crate::internals::inspect::read_image(&path).unwrap();
    let image = Image::read(&bytes).unwrap();
    let identity = image.identity().unwrap();
    assert_eq!(identity.architecture, "Aarch64");
    assert_eq!(identity.format, "MachO");
    assert_eq!(identity.executable, identity.slice);

    let start = image.address("Probe::Read").unwrap();
    assert_eq!(start, READ);
    let listing = image.disassemble(start, 4096).unwrap();
    assert_eq!(listing.place, "Probe::Read()");
    assert_eq!(listing.end, End::NextSymbol("_helper".into()));
    assert_eq!(listing.rows.len(), 8);
    assert!(notes_at(&listing, READ + 0x8).contains(&Note::String("entity_offset".into())));
    assert_eq!(
        notes_at(&listing, READ + 0xc),
        [Note::Target {
            address: HELPER,
            place: "_helper".into()
        }]
    );

    assert_eq!(
        crate::Native::open(&path).err(),
        Some(crate::OpenError::UnknownTarget)
    );
}

#[test]
fn an_unread_fixup_format_leaves_symbols_strings_and_code() {
    let bytes = support::macho_image(1);
    let image = Image::read(&bytes).unwrap();

    let diagnostic = image.pointer_resolution().unwrap_err();
    assert!(
        diagnostic.contains("chained pointer format 1"),
        "{diagnostic}"
    );
    assert_eq!(image.symbols("Probe"), [(READ, "Probe::Read()")]);
    let references = image.string_references("entity_offset");
    assert_eq!(references.len(), 1);
    assert_eq!(
        (references[0].string_address, references[0].at),
        (STRING, READ + 0x8)
    );

    let listing = image.disassemble(READ, 4096).unwrap();
    assert!(!has_pointer(&listing));
    assert_eq!(
        notes_at(&listing, READ + 0x14),
        [Note::Address {
            address: SLOT,
            place: "__DATA,__data".into()
        }]
    );
    assert_eq!(image.slots(SLOT, 1), Err(diagnostic));

    let inventory = inventory::read(&bytes).unwrap();
    assert!(matches!(
        fixups::read(&inventory),
        Err(FixupDiagnostic::PointerFormat { format: 1, .. })
    ));
    assert!(discovery::read(&bytes).is_err());
}

#[test]
fn a_read_fixup_format_resolves_slots() {
    let bytes = support::macho_image(6);
    let image = Image::read(&bytes).unwrap();

    assert_eq!(image.pointer_resolution(), Ok(()));
    let listing = image.disassemble(READ, 4096).unwrap();
    assert!(
        notes_at(&listing, READ + 0x14).contains(&Note::Pointer(format!("{HELPER:#x} _helper")))
    );
    assert_eq!(
        image.slots(SLOT, 2).unwrap()[0].holds,
        format!("{HELPER:#x} _helper")
    );

    let input = discovery::read(&bytes).unwrap();
    assert_eq!(input.pointers.get(&SLOT), Some(&HELPER));
}

#[test]
fn an_unread_fixup_header_names_its_values() {
    let mut bytes = support::macho_image(6);
    bytes[support::IMAGE_FIXUPS_OFFSET] = 1;
    let inventory = inventory::read(&bytes).unwrap();

    assert_eq!(
        fixups::read(&inventory).err(),
        Some(FixupDiagnostic::Header {
            version: 1,
            imports_format: 3,
            symbols_format: 0
        })
    );
}

#[test]
fn an_image_without_fixups_says_so() {
    let bytes = support::macho(&support::code());
    let image = Image::read(&bytes).unwrap();

    assert_eq!(
        image.pointer_resolution(),
        Err("the image has no chained fixups".into())
    );
}

fn listing_of(words: &[u32]) -> Listing {
    let code: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let bytes = support::macho(&code);
    let image = Image::read(&bytes).unwrap();

    image.disassemble(0x1000, 4096).unwrap()
}

fn formed_addresses(listing: &Listing) -> Vec<u64> {
    listing
        .rows
        .iter()
        .flat_map(|row| &row.notes)
        .filter_map(|note| match note {
            Note::Address { address, .. } => Some(*address),
            _ => None,
        })
        .collect()
}

#[test]
fn a_page_is_forgotten_across_calls_branches_and_writes() {
    const ADRP_X8: u32 = 0xb000_0008; // adrp x8, 0x2000
    const ADD_X8: u32 = 0x9100_4108; // add x8, x8, #0x10

    assert_eq!(formed_addresses(&listing_of(&[ADRP_X8, ADD_X8])), [0x2010]);
    let call = [ADRP_X8, 0x9400_0010, ADD_X8]; // bl
    let branch = [ADRP_X8, 0x5400_0021, ADD_X8]; // b.ne to the add
    let write = [ADRP_X8, 0xd280_0008, ADD_X8]; // mov x8, #0
    let post_index = [ADRP_X8, 0xf800_8500, ADD_X8]; // str x0, [x8], #8
    let pre_index = [ADRP_X8, 0xf840_8d01, ADD_X8]; // ldr x1, [x8, #8]!
    assert_eq!(listing_of(&post_index).rows[1].operands, "x0,[x8],#8");
    assert_eq!(listing_of(&pre_index).rows[1].operands, "x1,[x8,#8]!");
    for words in [&call[..], &branch, &write, &post_index, &pre_index] {
        assert!(
            formed_addresses(&listing_of(words)).is_empty(),
            "{words:x?}"
        );
    }
}

#[test]
fn indirect_branches_and_undecoded_words_are_labelled() {
    let listing = listing_of(&[0xd61f_0100, 0xffff_ffff]); // br x8, then data

    assert_eq!(listing.rows[0].notes, [Note::IndirectUnresolved]);
    assert_eq!(listing.rows[1].operation, ".word");
    assert_eq!(listing.rows[1].notes, [Note::Undecoded]);
}

#[test]
fn invalid_starts_and_limits_are_errors() {
    let bytes = support::macho(&support::code());
    let image = Image::read(&bytes).unwrap();

    for start in [0, 0xffc, 0x1002, 0x1000 + 44, u64::MAX] {
        assert!(image.disassemble(start, 4096).is_err(), "{start:#x}");
    }
    for limit in [0, 6] {
        assert!(image.disassemble(0x1000, limit).is_err(), "{limit}");
    }
    assert_eq!(image.disassemble(0x1000, 8).unwrap().end, End::Limit);
}

#[test]
fn a_start_inside_a_function_names_its_symbol() {
    let bytes = support::macho_image(6);
    let image = Image::read(&bytes).unwrap();
    let listing = image.disassemble(READ + 0x10, 4096).unwrap();

    assert!(!listing.at_symbol);
    assert_eq!(listing.place, "Probe::Read()+0x10");
    assert!(listing.to_string().contains("inside Probe::Read()+0x10"));
}

#[test]
fn a_name_without_parameters_matches_functions_only() {
    assert!(is_parameter_list("(CReader&, int)"));
    assert!(is_parameter_list("() const"));
    assert!(is_parameter_list("(void (*)(int))"));
    assert!(!is_parameter_list("()::s_pTokenArray"));
    assert!(!is_parameter_list("::Nested()"));
    assert!(!is_parameter_list("(unbalanced"));
}

#[test]
fn a_slot_range_past_the_address_space_is_an_error() {
    let bytes = support::macho_image(6);
    let image = Image::read(&bytes).unwrap();
    let last = u64::MAX - 7;

    assert!(image.slots(last, 2).is_err());
    assert_eq!(image.slots(last, 1).unwrap().len(), 1);
}

#[test]
fn a_base_register_that_is_only_read_keeps_its_page() {
    assert_eq!(writeback_base("x0,[x8,#8]"), None);
    assert_eq!(writeback_base("x0,[x8]"), None);
    assert_eq!(writeback_base("x0,[x8,#8]!"), Some(8));
    assert_eq!(writeback_base("x0,x1,[sp],#16"), None);
    assert_eq!(writeback_base("x0,[x9],#16"), Some(9));
}

#[test]
fn a_stop_is_placed_at_its_instruction_function_and_entry() {
    use crate::engine::analysis::stop::{Obstacle, Unknown};

    let bytes = support::macho_image(6);
    let image = Image::read(&bytes).unwrap();
    let stop = Stop {
        instruction: READ + 0x14,
        entry: READ,
        obstacle: Obstacle::Unknown(Unknown::Register(8)),
    };
    let placed = image.place_stop(stop);

    assert_eq!(placed.place, "Probe::Read()+0x14");
    assert_eq!(placed.function.as_deref(), Some("Probe::Read()"));
    assert_eq!(placed.entry, "Probe::Read()");
    let row = placed.row.as_ref().expect("a text instruction");
    assert_eq!(
        (row.operation.as_str(), row.operands.as_str()),
        ("ldr", "x8,[x8]")
    );
    assert_eq!(
        placed.to_string(),
        format!("x8 unknown\n  {row}\n  at Probe::Read()+0x14; entered at {READ:#x} Probe::Read()")
    );

    let outside = image.place_stop(Stop {
        instruction: SLOT,
        ..stop
    });
    assert_eq!(outside.function, None);
    assert_eq!(outside.row, None);
}
