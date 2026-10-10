use super::super::lexer::{LexerInput, character_tables, token_boundary};
use super::*;
use crate::engine::analysis::evaluate::ReadOnlyData;
use crate::engine::analysis::references::shapes::canonical_local;
use crate::engine::analysis::references::tests::Image;

/// The M452 first-byte, word-end and push-back tables at `0x102da1bc0`, `0x102da1c1d` and
/// `0x102da1c7a`, for bytes `!` through `}`.
const M452_TABLES: [&str; 3] = [
    "02040006060606080a06060c0606060606060606060606060606000e151006\
     06060606060606060606060606060606060606060606060606060606060606\
     06060606060606060606060606060606060606060606060606060606120614",
    "15151500000000151500001500000000000000000000000000001515151500\
     00000000000000000000000000000000000000000000000000000000000000\
     00000000000000000000000000000000000000000000000000000000150015",
    "00000002020202000002020002020202020202020202020202020000000002\
     02020202020202020202020202020202020202020202020202020202020202\
     02020202020202020202020202020202020202020202020202020202000200",
];

fn m452_tables() -> [Vec<u8>; 3] {
    M452_TABLES.map(|table| {
        (0..table.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&table[index..index + 2], 16).unwrap())
            .collect()
    })
}

fn placed(addresses: &[u64], tables: [Vec<u8>; 3]) -> ReadOnlyData {
    ReadOnlyData::new(addresses.iter().copied().zip(tables).collect())
}

/// Three unnamed table bases, then one dispatch through the first table.
fn table_rows() -> Vec<Instruction> {
    let bytes = arm64!(at 0x1000;
        adrp x24, extern 0x8000;
        add x24, x24, #0x0; // first-byte table
        adrp x27, extern 0x8000;
        add x27, x27, #0x100; // word-end table
        adrp x9, extern 0x8000;
        add x9, x9, #0x200; // push-back table
        adr x10, ->handlers;
        ldrb w11, [x24, x8];
        add x10, x10, x11, lsl #2;
        br x10;
        ->handlers:;
        ret
    );
    decode_arm64(&bytes, 0x1000).unwrap()
}

fn tables_of(rows: &[Instruction], data: &ReadOnlyData) -> Result<(), &'static str> {
    let lines = canonical_local(rows, &BTreeMap::new()).unwrap();
    character_tables(rows, &lines, data).map_err(|obstacle| obstacle.reason)
}

#[test]
fn authored_table_bases_hold_the_established_classification() {
    let rows = table_rows();
    let addresses = [0x8000, 0x8100, 0x8200];
    assert_eq!(tables_of(&rows, &placed(&addresses, m452_tables())), Ok(()));

    for (table, byte, entry) in [
        (1, b'-', 0x15), // `-` would end a word
        (2, b'}', 0x02), // `}` would not be pushed back
        (0, b';', 0x06), // `;` would start a word
    ] {
        let mut tables = m452_tables();
        tables[table][usize::from(byte - b'!')] = entry;
        assert_eq!(
            tables_of(&rows, &placed(&addresses, tables)),
            Err("numeric-lexer-table"),
            "{}",
            byte as char
        );
    }

    let [first, word_end, _] = m452_tables();
    let missing = ReadOnlyData::new(vec![(0x8000, first), (0x8100, word_end)]);
    assert_eq!(tables_of(&rows, &missing), Err("numeric-lexer-table"));
}

fn production_lexer() -> (LexerInput, Vec<u64>) {
    let mut image = Image::default();
    image.add(
        "GetTok",
        include_str!("../shapes/lexer.txt"),
        &[("source_file", "\"lexer.cpp\"")],
    );
    let addresses = image.unnamed().to_vec();
    let input = LexerInput {
        body: image.function("GetTok").to_vec(),
        names: image.names().clone(),
        data: placed(&addresses, m452_tables()),
    };
    (input, addresses)
}

#[test]
fn the_production_shape_establishes_the_boundary_only_with_its_input_slots_and_dispatch() {
    let (input, addresses) = production_lexer();
    assert_eq!(addresses.len(), 3);
    assert_eq!(token_boundary(&input), Ok(()));

    let mut changed_slot = production_lexer().0;
    let validity = changed_slot
        .body
        .iter_mut()
        .find(|row| row.operation == "ldr" && row.operands.ends_with(",#0x58]"))
        .unwrap();
    validity.operands = validity.operands.replace("#0x58]", "#0x60]");
    assert_eq!(
        token_boundary(&changed_slot).map_err(|obstacle| obstacle.reason),
        Err("numeric-lexer-shape")
    );

    let mut moved_base = production_lexer().0;
    let base = moved_base
        .body
        .iter_mut()
        .find(|row| row.operation == "adr")
        .unwrap();
    let (register, target) = base.operands.split_once(",#0x").unwrap();
    let target = u64::from_str_radix(target, 16).unwrap() + 4;
    base.operands = format!("{register},#{target:#x}");
    assert_eq!(
        token_boundary(&moved_base).map_err(|obstacle| obstacle.reason),
        Err("numeric-lexer-shape")
    );
}
