use super::*;
use crate::GrammarProperty::{Known, Unresolved};
use crate::engine::analysis::{assembler::arm64, decode::decode_arm64};

fn scan_rows() -> Vec<Instruction> {
    let bytes = arm64!(at 0x1000;
        sub sp, sp, #0x20;
        stp x29, x30, [sp, #0x10];
        add x29, sp, #0x10;
        ldr x0, [x0, #0x10];
        str x1, [sp];
        adrp x1, extern 0x8000;
        add x1, x1, #0x20;
        bl extern 0x9000;
        cmp w0, #0;
        cset w0, ne;
        ldp x29, x30, [sp, #0x10];
        add sp, sp, #0x20;
        ret
    );
    decode_arm64(&bytes, 0x1000).unwrap()
}

fn names(format: &str) -> BTreeMap<u64, String> {
    BTreeMap::from([(0x8020, format!("{format:?}")), (0x9000, "scan".into())])
}

#[test]
fn direct_scan_proves_storage_and_partial_syntax_without_universal_acceptance() {
    for (format, representation, width, sign, scale) in [
        (
            "%i",
            NumericRepresentation::Integer,
            32,
            NumericSignedness::Signed,
            Some(1),
        ),
        (
            "%u",
            NumericRepresentation::Integer,
            32,
            NumericSignedness::Unsigned,
            Some(1),
        ),
        (
            "%lld",
            NumericRepresentation::Integer,
            64,
            NumericSignedness::Signed,
            Some(1),
        ),
        (
            "%llu",
            NumericRepresentation::Integer,
            64,
            NumericSignedness::Unsigned,
            Some(1),
        ),
        (
            "%f",
            NumericRepresentation::BinaryFloat,
            32,
            NumericSignedness::Signed,
            None,
        ),
    ] {
        let fact = token_conversion(&scan_rows(), &names(format), 0x10).unwrap();
        assert_eq!(fact.representation, Known(representation));
        assert_eq!(fact.width_bits, Known(width));
        assert_eq!(fact.signedness, Known(sign));
        assert_eq!(fact.scale, Known(scale));
        assert!(
            matches!(fact.literal_syntax, GrammarProperty::Partial(ref forms) if !forms.is_empty())
        );
        assert_eq!(
            fact.accepted_range,
            if format == "%i" {
                signed_32_range()
            } else {
                Unresolved
            }
        );
        assert_eq!(fact.clamp, Known(None));
    }
}

#[test]
fn scoped_token_reader_keeps_conversion_limits_and_fails_closed() {
    let fact = analyze_token(&TokenInput {
        body: scan_rows(),
        names: names("%i"),
        token_text_offset: 0x10,
    });
    assert!(matches!(fact.conversion, GrammarProperty::Partial(Some(_))));
    assert!(fact.gaps.iter().any(|gap| gap.reason == "numeric-overflow"));

    let missing = analyze_token(&TokenInput {
        body: scan_rows(),
        names: BTreeMap::new(),
        token_text_offset: 0x10,
    });
    assert_eq!(missing.conversion, GrammarProperty::Unresolved);
}

#[test]
fn wrong_destination_unknown_calls_and_incomplete_checks_do_not_prove_storage() {
    for (index, operation, operands) in [
        (4, "str", "x2,[sp]"),
        (7, "bl", "#0x9004"),
        (9, "b.eq", "#0x1010"),
        (8, "cmp", "w0,#0x7fffffff"),
        (12, "br", "x8"),
    ] {
        let mut rows = scan_rows();
        rows[index].operation = operation.into();
        rows[index].operands = operands.into();
        assert!(token_conversion(&rows, &names("%i"), 0x10).is_none());
    }
    assert!(token_conversion(&scan_rows(), &names("%s"), 0x10).is_none());
    assert!(token_conversion(&scan_rows(), &names("%i"), 0x18).is_none());
}

#[test]
fn conflicting_and_unresolved_paths_cannot_inherit_scan_properties() {
    let mut rows = scan_rows();
    rows.insert(
        0,
        Instruction {
            bytes: [0; 4],
            address: 0xffc,
            operation: "cbz".into(),
            operands: "w8,#0x1010".into(),
        },
    );
    assert!(token_conversion(&rows, &names("%i"), 0x10).is_none());
}

#[test]
fn old_reader_answers_default_to_unknown_and_exact_bounds_round_trip() {
    let reader: crate::Reader = serde_json::from_str(r#"{"id":null,"kind":"Integer"}"#).unwrap();
    assert_eq!(reader.numeric, Unresolved);
    for bound in [
        crate::NumericBound::Signed(i64::MIN),
        crate::NumericBound::Unsigned(u64::MAX),
    ] {
        let serialized = serde_json::to_string(&bound).unwrap();
        assert_eq!(
            serde_json::from_str::<crate::NumericBound>(&serialized).unwrap(),
            bound
        );
    }
    let mut reader = reader;
    reader.numeric = GrammarProperty::Partial(Some(
        token_conversion(&scan_rows(), &names("%i"), 0x10).unwrap(),
    ));
    assert_eq!(
        serde_json::from_str::<crate::Reader>(&serde_json::to_string(&reader).unwrap()).unwrap(),
        reader
    );
}

#[test]
#[ignore = "requires the exact supported executable through STELLARIS_PATH"]
fn m45_numeric_reader_static_parity() {
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let facts = crate::internals::numeric_readers::run(&native).unwrap();
    for (name, width, scale, range) in [
        ("CToken::ReadValue(int&) const", 32, 1, signed_32_range()),
        (
            "CToken::ReadValue(CFixedPoint&) const",
            64,
            100_000,
            fixed_point_range(100_000),
        ),
    ] {
        let reader = &facts.token_readers[name];
        let GrammarProperty::Partial(Some(conversion)) = &reader.conversion else {
            panic!("{name}: {:?}", reader.conversion);
        };
        assert_eq!(conversion.width_bits, Known(width), "{name}");
        assert_eq!(conversion.scale, Known(Some(scale)), "{name}");
        assert_eq!(conversion.accepted_range, range, "{name}");
    }
    let actual = serde_json::to_string_pretty(&facts).unwrap();
    if let Some(path) = std::env::var_os("NATIVE_NUMERIC_REPORT") {
        std::fs::write(path, &actual).unwrap();
    }
    let compact: BTreeMap<_, _> = facts
        .readers
        .iter()
        .map(|(name, reader)| {
            let GrammarProperty::Partial(Some(conversion)) = &reader.conversion else {
                panic!("{name}: {:?}", reader.conversion);
            };
            (
                name,
                serde_json::json!([
                    conversion.representation,
                    conversion.width_bits,
                    conversion.signedness,
                    conversion.scale,
                    conversion.literal_syntax,
                    conversion.accepted_range,
                    conversion.clamp,
                    reader.gaps.iter().map(|gap| gap.reason).collect::<Vec<_>>()
                ]),
            )
        })
        .collect();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/expected/numeric-m45/readers.json"
    ))
    .unwrap();
    assert_eq!(serde_json::to_value(compact).unwrap(), expected);
    let entry: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/expected/numeric-m45/modifier-entry.json"
    ))
    .unwrap();
    assert_eq!(serde_json::to_value(&facts.modifier_entry).unwrap(), entry);
}

fn decimal_rows() -> (Vec<Instruction>, BTreeMap<u64, String>) {
    let bytes = arm64!(at 0x2000;
        sub sp,sp,#0x50;
        stp x22,x21,[sp,#0x20];
        stp x20,x19,[sp,#0x30];
        stp x29,x30,[sp,#0x40];
        add x29,sp,#0x40;
        mov x19,x1;
        mov x21,x0;
        stp xzr,xzr,[sp,#0x10];
        ldr x0,[x0,#0x10];
        add x8,sp,#0x18;
        str x8,[sp];
        adrp x1, extern 0x8000;
        add x1,x1,#0x20;
        bl extern 0x9000;
        mov x20,x0;
        cbz w0,extern 0x2124;
        ldr x0,[x21,#0x10];
        mov w1,#0x2e;
        bl extern 0x9010;
        cbz x0,extern 0x2110;
        orr w8,wzr,#0x30303030;
        str w8,[sp,#8];
        mov w8,#0x30;
        strh w8,[sp,#0xc];
        ldrb w8,[x0,#1];
        cbz w8,extern 0x209c;
        strb w8,[sp,#8];
        ldrb w8,[x0,#2];
        cbz w8,extern 0x209c;
        strb w8,[sp,#9];
        ldrb w8,[x0,#3];
        cbz w8,extern 0x209c;
        strb w8,[sp,#0xa];
        ldrb w8,[x0,#4];
        cbz w8,extern 0x209c;
        strb w8,[sp,#0xb];
        ldrb w8,[x0,#5];
        cbz w8,extern 0x209c;
        strb w8,[sp,#0xc];
        mov x8,#0;
        add x10,sp,#8;
        mov x9,x8;
        add x8,x8,#1;
        ldrb w11,[x10,x9];
        cmp w11,#0x30;
        b.eq extern 0x20a4;
        add x8,sp,#8;
        add x0,x8,w9,uxtw;
        add x8,sp,#0x10;
        str x8,[sp];
        adrp x1, extern 0x8000;
        add x1,x1,#0x20;
        bl extern 0x9000;
        ldp x9,x8,[sp,#0x10];
        tbz x8,#0x3f,extern 0x20e4;
        neg x9,x9;
        str x9,[sp,#0x10];
        mov w10,#0x86a0;
        movk w10,#1,lsl #16;
        madd x9,x8,x10,x9;
        str x9,[x19];
        ldr x10,[x21,#0x10];
        ldrb w10,[x10];
        cmp w10,#0x2d;
        ccmp x8,#0,#0,eq;
        b.ne extern 0x2124;
        neg x8,x9;
        b extern 0x2120;
        ldr x8,[sp,#0x18];
        mov w9,#0x86a0;
        movk w9,#1,lsl #16;
        mul x8,x8,x9;
        str x8,[x19];
        cmp w20,#0;
        cset w0,ne;
        ldp x29,x30,[sp,#0x40];
        ldp x20,x19,[sp,#0x30];
        ldp x22,x21,[sp,#0x20];
        add sp,sp,#0x50;
        ret
    );
    (
        decode_arm64(&bytes, 0x2000).unwrap(),
        BTreeMap::from([
            (0x8020, "\"%lli\"".into()),
            (0x9000, "scan".into()),
            (0x9010, "find_character".into()),
        ]),
    )
}

fn binary_rows() -> (Vec<Instruction>, BTreeMap<u64, String>) {
    let bytes = arm64!(at 0x3000;
        sub sp,sp,#0x40;
        stp x20,x19,[sp,#0x20];
        stp x29,x30,[sp,#0x30];
        add x29,sp,#0x30;
        mov x19,x1;
        ldr w8,[x0];
        cmp w8,#0x167;
        b.ne extern 0x3040;
        ldr x0,[x0,#0x10];
        bl extern 0x9020;
        str x0,[x19];
        mov w0,#1;
        ldp x29,x30,[sp,#0x30];
        ldp x20,x19,[sp,#0x20];
        add sp,sp,#0x40;
        ret;
        stp xzr,xzr,[sp,#0x10];
        ldr x0,[x0,#0x10];
        ldrb w20,[x0];
        add x8,sp,#0x10;
        add x9,sp,#0x18;
        stp x9,x8,[sp];
        adrp x1, extern 0x8000;
        add x1,x1,#0x28;
        bl extern 0x9000;
        ldr x8,[sp,#0x18];
        ldr d0,[sp,#0x10];
        mov x9,#0x40e0000000000000;
        fmov d1,x9;
        fcmp d0,#0.0;
        fmov d2,#-0.50000000;
        fmov d3,#0.50000000;
        fcsel d4,d3,d2,ge;
        fmadd d1,d0,d1,d4;
        fcvtzs x9,d1;
        lsl x8,x8,#0xf;
        sub x9,x8,x9;
        mov x10,#0x40e0000000000000;
        fmov d1,x10;
        fcmp d0,#0.0;
        fcsel d2,d3,d2,ge;
        fmadd d0,d0,d1,d2;
        fcvtzs x10,d0;
        add x8,x8,x10;
        cmp w20,#0x2d;
        csel x0,x9,x8,eq;
        str x0,[x19];
        mov w0,#1;
        ldp x29,x30,[sp,#0x30];
        ldp x20,x19,[sp,#0x20];
        add sp,sp,#0x40;
        ret
    );
    (
        decode_arm64(&bytes, 0x3000).unwrap(),
        BTreeMap::from([
            (0x9020, "decimal_integer".into()),
            (0x8028, "\"%lld%lf\"".into()),
            (0x9000, "scan".into()),
        ]),
    )
}

#[test]
fn fixed_point_facts_follow_both_integer_and_fractional_paths_to_the_store() {
    for (rows, names, scale) in [
        {
            let (rows, names) = decimal_rows();
            (rows, names, 100000)
        },
        {
            let (rows, names) = binary_rows();
            (rows, names, 32768)
        },
    ] {
        let fact = token_conversion(&rows, &names, 0x10).unwrap();
        assert_eq!(fact.width_bits, Known(64));
        assert_eq!(fact.scale, Known(Some(scale)));
        assert_eq!(fact.signedness, Known(NumericSignedness::Signed));
        assert!(
            matches!(fact.literal_syntax, GrammarProperty::Partial(ref forms) if !forms.is_empty())
        );
        assert_eq!(fact.accepted_range, fixed_point_range(scale));
    }
}

#[test]
fn changed_scale_keeps_storage_but_not_a_false_uniform_scale() {
    let (mut rows, names) = decimal_rows();
    for row in &mut rows {
        if row.operation == "mov" {
            row.operands = row.operands.replace("#0x86a0", "#0x86a1");
        }
    }
    let fact = token_conversion(&rows, &names, 0x10).unwrap();
    assert_eq!(fact.width_bits, Known(64));
    assert_eq!(fact.scale, Unresolved);
    assert_eq!(fact.accepted_range, Unresolved);

    let (mut rows, names) = binary_rows();
    let shift = rows.iter_mut().find(|row| row.operation == "lsl").unwrap();
    shift.operands = shift.operands.replace("#0xf", "#0xe");
    let fact = token_conversion(&rows, &names, 0x10).unwrap();
    assert_eq!(fact.width_bits, Known(64));
    assert_eq!(fact.scale, Unresolved);
    assert_eq!(fact.accepted_range, Unresolved);
}

#[test]
fn raw_token_path_unknown_call_or_wrong_store_prevents_a_complete_shape_match() {
    let (rows, names) = binary_rows();
    for operation in ["bl", "str"] {
        let mut changed = rows.clone();
        let row = changed
            .iter_mut()
            .find(|row| row.operation == operation)
            .unwrap();
        row.operands = if operation == "bl" {
            "#0x9030"
        } else {
            "x0,[x20]"
        }
        .into();
        assert!(token_conversion(&changed, &names, 0x10).is_none());
    }
}

#[test]
fn matching_integer_and_fractional_scale_changes_are_derived_from_the_instructions() {
    let (mut rows, names) = binary_rows();
    for row in &mut rows {
        if row.operation == "lsl" {
            row.operands = row.operands.replace("#0xf", "#0xe");
        }
        if row.operation == "mov" {
            row.operands = row
                .operands
                .replace("#0x40e0000000000000", "#0x40d0000000000000");
        }
    }
    assert_eq!(
        token_conversion(&rows, &names, 0x10).unwrap().scale,
        Known(Some(16384))
    );
}

#[test]
fn halfword_conversion_proves_width_without_inferring_sign_from_the_callee_type() {
    let bytes = arm64!(at 0x1000;
        sub sp,sp,#0x30;
        stp x20,x19,[sp,#0x10];
        stp x29,x30,[sp,#0x20];
        add x29,sp,#0x20;
        mov x19,x1;
        ldr x0,[x0,#0x10];
        add x8,sp,#0xc;
        str x8,[sp];
        adrp x1,extern 0x8000;
        add x1,x1,#0x20;
        bl extern 0x9000;
        ldrh w8,[sp,#0xc];
        cmp w0,#0;
        csel w8,wzr,w8,eq;
        cset w0,ne;
        strh w8,[x19];
        ldp x29,x30,[sp,#0x20];
        ldp x20,x19,[sp,#0x10];
        add sp,sp,#0x30;
        ret
    );
    let rows = decode_arm64(&bytes, 0x1000).unwrap();
    for format in ["%d", "%u"] {
        let fact = token_conversion(&rows, &names(format), 0x10).unwrap();
        assert_eq!(fact.width_bits, Known(16));
        assert_eq!(fact.signedness, Unresolved);
    }
    assert!(token_conversion(&rows, &names("%lld"), 0x10).is_none());
    for (operation, replacement) in [("ldrh", "ldrsh"), ("strh", "strb")] {
        let mut changed = rows.clone();
        changed
            .iter_mut()
            .find(|row| row.operation == operation)
            .unwrap()
            .operation = replacement.into();
        assert!(token_conversion(&changed, &names("%d"), 0x10).is_none());
    }
}

#[test]
fn byte_conversion_keeps_narrowing_separate_from_parser_range() {
    let bytes = arm64!(at 0x1000;
        sub sp,sp,#0x30;
        stp x20,x19,[sp,#0x10];
        stp x29,x30,[sp,#0x20];
        add x29,sp,#0x20;
        mov x19,x1;
        str wzr,[sp,#0xc];
        ldr x0,[x0,#0x10];
        add x8,sp,#0xc;
        str x8,[sp];
        adrp x1,extern 0x8000;
        add x1,x1,#0x20;
        bl extern 0x9000;
        ldrb w8,[sp,#0xc];
        cmp w0,#0;
        csel w8,wzr,w8,eq;
        cset w0,ne;
        strb w8,[x19];
        ldp x29,x30,[sp,#0x20];
        ldp x20,x19,[sp,#0x10];
        add sp,sp,#0x30;
        ret
    );
    let rows = decode_arm64(&bytes, 0x1000).unwrap();
    let fact = token_conversion(&rows, &names("%d"), 0x10).unwrap();
    assert_eq!(fact.width_bits, Known(8));
    assert_eq!(fact.signedness, Unresolved);
    assert_eq!(fact.accepted_range, Unresolved);
    let mut changed = rows;
    changed
        .iter_mut()
        .find(|row| row.operation == "ldrb")
        .unwrap()
        .operation = "ldrsb".into();
    assert!(token_conversion(&changed, &names("%d"), 0x10).is_none());
}

mod boundaries;
mod modifier;

#[test]
fn scanner_formats_prove_only_partial_literal_families() {
    use NumericLiteralSyntax::{DecimalFraction, DecimalInteger, Exponent, RadixPrefixedInteger};
    for (format, forms) in [
        ("%i", vec![DecimalInteger, RadixPrefixedInteger]),
        ("%lli", vec![DecimalInteger, RadixPrefixedInteger]),
        ("%d", vec![DecimalInteger]),
        ("%u", vec![DecimalInteger]),
        ("%lld", vec![DecimalInteger]),
        ("%llu", vec![DecimalInteger]),
        ("%f", vec![DecimalInteger, DecimalFraction, Exponent]),
    ] {
        let fact = token_conversion(&scan_rows(), &names(format), 0x10).unwrap();
        assert_eq!(fact.literal_syntax, GrammarProperty::Partial(forms));
        assert_eq!(
            fact.accepted_range,
            if format == "%i" {
                signed_32_range()
            } else {
                Unresolved
            }
        );
        assert_eq!(fact.clamp, Known(None));
    }
    let (rows, names) = binary_rows();
    let fact = token_conversion(&rows, &names, 0x10).unwrap();
    assert_eq!(
        fact.literal_syntax,
        GrammarProperty::Partial(vec![DecimalInteger, DecimalFraction])
    );
}

#[test]
fn raw_value_mode_adds_its_scanner_forms_without_replacing_fixed_point_scale() {
    use NumericLiteralSyntax::{DecimalFraction, DecimalInteger, RadixPrefixedInteger};
    let (rows, token_names) = binary_rows();
    let ordinary = token_conversion(&rows, &token_names, 0x10).unwrap();
    let raw = token_conversion(&scan_rows(), &names("%lli"), 0x10).unwrap();
    let (combined, gap) = fixed_paths(Some(ordinary), Some(raw));
    let combined = combined.unwrap();
    assert_eq!(gap, None);
    assert_eq!(combined.scale, Known(Some(32768)));
    assert_eq!(
        combined.literal_syntax,
        GrammarProperty::Partial(vec![DecimalInteger, DecimalFraction, RadixPrefixedInteger])
    );
}

#[test]
fn an_unproved_raw_path_keeps_partial_storage_and_syntax_but_cannot_prove_no_clamp() {
    let (rows, names) = decimal_rows();
    let ordinary = token_conversion(&rows, &names, 0x10).unwrap();
    let forms = ordinary.literal_syntax.clone();
    let (combined, gap) = fixed_paths(Some(ordinary), None);
    let combined = combined.unwrap();
    assert_eq!(gap, Some("numeric-raw-conversion"));
    assert_eq!(combined.width_bits, GrammarProperty::Partial(64));
    assert_eq!(combined.literal_syntax, forms);
    assert_eq!(combined.clamp, Unresolved);
    let raw = token_conversion(&scan_rows(), &self::names("%lld"), 0x10).unwrap();
    let (combined, gap) = fixed_paths(None, Some(raw));
    let combined = combined.unwrap();
    assert_eq!(gap, Some("numeric-token-shape"));
    assert_eq!(combined.scale, Unresolved);
    assert_eq!(
        combined.literal_syntax,
        GrammarProperty::Partial(vec![NumericLiteralSyntax::DecimalInteger])
    );
}

#[test]
fn an_incompatible_raw_conversion_cannot_add_literal_forms() {
    let (rows, token_names) = binary_rows();
    let ordinary = token_conversion(&rows, &token_names, 0x10).unwrap();
    let forms = ordinary.literal_syntax.clone();
    let raw = token_conversion(&scan_rows(), &names("%f"), 0x10).unwrap();
    let (combined, gap) = fixed_paths(Some(ordinary), Some(raw));
    let combined = combined.unwrap();
    assert_eq!(gap, Some("numeric-raw-storage"));
    assert_eq!(combined.literal_syntax, forms);
    assert_eq!(combined.clamp, Unresolved);
}

fn signed_32_range() -> GrammarProperty<Box<NumericRange>> {
    Known(Box::new(NumericRange {
        minimum: Known(NumericBound::Signed(-2147483648)),
        maximum: Known(NumericBound::Signed(2147483647)),
    }))
}

fn fixed_point_range(scale: u64) -> GrammarProperty<Box<NumericRange>> {
    Known(Box::new(NumericRange {
        minimum: Known(NumericBound::Rational {
            numerator: -9223372036854775808,
            denominator: scale,
        }),
        maximum: Known(NumericBound::Rational {
            numerator: 9223372036854775807,
            denominator: scale,
        }),
    }))
}
