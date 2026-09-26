use super::*;
use crate::engine::analysis::stop::Obstacle;

fn rows(base: u64, instructions: &[(&str, String)]) -> Vec<Instruction> {
    instructions
        .iter()
        .enumerate()
        .map(|(index, (operation, operands))| Instruction {
            address: base + index as u64 * 4,
            bytes: [0; 4],
            operation: (*operation).into(),
            operands: operands.clone(),
        })
        .collect()
}

fn input(layout: Layout) -> Input {
    let documentation = rows(
        0x1000,
        &[
            ("adrp", "x9,#0x8000".into()),
            (
                "ldrsw",
                format!("x8,[x9,#{:#x}]", layout.array_count_offset),
            ),
            ("cbz", "w8,#0x1040".into()),
            ("ldr", format!("x26,[x9,#{:#x}]", layout.array_data_offset)),
            ("mov", format!("w9,#{:#x}", layout.definition_stride)),
            ("madd", "x27,x8,x9,x26".into()),
            ("ldr", format!("w0,[x26,#{:#x}]", layout.token_offset)),
            ("bl", "#0x2000".into()),
            ("ldr", format!("w0,[x26,#{:#x}]", layout.mask_offset)),
            ("bl", "#0x4000".into()),
            ("add", format!("x26,x26,#{:#x}", layout.definition_stride)),
            ("cmp", "x26,x27".into()),
            ("b.ne", "#0x1018".into()),
            ("nop", "".into()),
            ("nop", "".into()),
            ("nop", "".into()),
            ("bl", "#0x6000".into()),
            ("ret", "".into()),
        ],
    );
    let get_string = rows(
        0x2000,
        &[
            ("mov", "x19,x0".into()),
            ("adrp", "x8,#0x9000".into()),
            ("add", "x8,x8,#0".into()),
            ("ldaprb", "w8,[x8]".into()),
            ("tbz", "w8,#0,#0x3000".into()),
            ("adrp", format!("x8,#{:#x}", layout.lookup)),
            ("add", format!("x8,x8,#{:#x}", layout.array_count_offset)),
            ("ldp", "w9,w8,[x8]".into()),
            ("cmp", "w9,w8".into()),
            ("b.eq", "#0x202c".into()),
            ("bl", "#0x5000".into()),
            ("adrp", format!("x8,#{:#x}", layout.lookup)),
            ("ldr", format!("x8,[x8,#{:#x}]", layout.array_data_offset)),
            ("mov", format!("w9,#{:#x}", layout.lookup_stride)),
            ("smaddl", "x0,w19,w9,x8".into()),
            ("ret", "".into()),
        ],
    );
    Input {
        documentation,
        get_string,
        definitions: 0x8000,
        category_name: 0x4000,
        rebuild_lookup: 0x5000,
        logger: 0x6000,
        pointers: BTreeMap::new(),
        data: ReadOnlyData::default(),
        strings: StringFunctions::default(),
        string_layout: StringLayout { flag_byte: 23 },
    }
}

fn layout() -> Layout {
    Layout {
        array_data_offset: 8,
        array_count_offset: 20,
        definition_stride: 152,
        token_offset: 120,
        mask_offset: 132,
        lookup: 0xa000,
        lookup_size: 0xa018,
        lookup_stride: 40,
    }
}

#[test]
fn derives_changed_headers_entries_and_lookup() {
    let original = layout();
    let changed = Layout {
        array_data_offset: 16,
        array_count_offset: 32,
        definition_stride: 184,
        token_offset: 124,
        mask_offset: 144,
        lookup: 0xb000,
        lookup_size: 0xb024,
        lookup_stride: 48,
    };
    for expected in [original, changed] {
        assert_eq!(derive(&input(expected)), Ok(expected));
    }
}

#[test]
fn refuses_unknown_instructions_and_unlabelled_fields() {
    let mut unknown = input(layout());
    unknown.documentation[8].operation = "unsupported".into();
    assert_eq!(
        derive(&unknown),
        Err(Unresolved::at(
            "instruction",
            0x1020,
            0x1000,
            Obstacle::Unsupported
        ))
    );
    let mut unlabelled = input(layout());
    unlabelled.documentation[8].operation = "mov".into();
    unlabelled.documentation[8].operands = "w0,#1".into();
    assert_eq!(
        derive(&unlabelled),
        Err(Unresolved::at(
            "modifier-mask-offset",
            0x1024,
            0x1000,
            Obstacle::Call
        ))
    );
}

#[test]
fn refuses_inconsistent_stride_count_and_lookup() {
    let mut stride = input(layout());
    stride.documentation[10].operands = "x26,x26,#0x94".into();
    assert!(derive(&stride).is_err());
    let mut count = input(layout());
    count.documentation[2].operation = "nop".into();
    count.documentation[2].operands.clear();
    assert_eq!(
        derive(&count),
        Err(Unresolved::new("modifier-array-header"))
    );
    let mut lookup = input(layout());
    lookup.get_string[14].operands = "x0,w19,w19,x8".into();
    assert_eq!(derive(&lookup), Err(Unresolved::new("lexer-index")));
    let mut rebuild = input(layout());
    rebuild.get_string[9].operation = "b.ne".into();
    assert_eq!(derive(&rebuild), Err(Unresolved::new("lexer-lookup-shape")));
}

#[test]
fn refuses_arithmetic_disguised_as_an_adjacent_field() {
    let mut changed = input(layout());
    // Insert into an existing spare instruction slot without moving branch destinations.
    changed.documentation[6].operands = "w0,[x26,#0x78]".into();
    changed.documentation[7].operation = "add".into();
    changed.documentation[7].operands = "w0,w0,#4".into();
    changed.documentation[8].operation = "bl".into();
    changed.documentation[8].operands = "#0x2000".into();
    changed.documentation[9].operation = "ldr".into();
    changed.documentation[9].operands = "w0,[x26,#0x84]".into();
    changed.documentation[10].operation = "bl".into();
    changed.documentation[10].operands = "#0x4000".into();
    changed.documentation[11].operation = "add".into();
    changed.documentation[11].operands = "x26,x26,#0x98".into();
    changed.documentation[12].operation = "cmp".into();
    changed.documentation[12].operands = "x26,x27".into();
    changed.documentation[13].operation = "b.ne".into();
    changed.documentation[13].operands = "#0x1018".into();
    assert_eq!(
        derive(&changed),
        Err(Unresolved::new("modifier-field-transformation"))
    );
}

#[test]
fn memory_operand_defaults_to_offset_zero_and_refuses_unreadable_parts() {
    assert_eq!(memory_operand("x8]"), Some((8, 0)));
    assert_eq!(memory_operand("x9,#0x18]"), Some((9, 0x18)));
    assert_eq!(memory_operand("x9,#24]"), Some((9, 24)));
    assert_eq!(memory_operand("x9,w8]"), None);
    assert_eq!(memory_operand("sp,#8]"), None);
    assert_eq!(memory_operand("x31]"), None);
    assert_eq!(memory_operand("x8,#8]!"), None);
}
