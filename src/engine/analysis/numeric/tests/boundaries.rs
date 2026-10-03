use super::*;

fn fixed_reader(binary: bool) -> ReaderInput {
    let wrapper = arm64!(at 0x4000;
        sub sp,sp,#0x50;
        stp x20,x19,[sp,#0x30];
        stp x29,x30,[sp,#0x40];
        add x29,sp,#0x40;
        mov x20,x1;
        mov x19,x0;
        ldr x0,[x0,#0x30];
        ldr x8,[x0];
        ldr x8,[x8,#0x20];
        blr x8; // unresolved raw-mode selector
        cbz w0,->ordinary;
        str xzr,[sp];
        add x0,x19,#0x278;
        mov x1,sp;
        bl extern 0xa010; // integer token conversion
        tbz w0,#0,->raw_failed;
        ldr x8,[sp];
        str x8,[x20]; // already-scaled value
        ->success:;
        ldp x29,x30,[sp,#0x40];
        ldp x20,x19,[sp,#0x30];
        add sp,sp,#0x50;
        ret;
        ->ordinary:;
        add x0,x19,#0x278;
        mov x1,x20;
        bl extern 0xa000;
        tbnz w0,#0,->success;
        adrp x1,extern 0x8000;
        add x1,x1,#0x40;
        add x0,sp,#8;
        bl extern 0xa020;
        add x1,sp,#8;
        mov x0,x19;
        bl extern 0xa030;
        b ->cleanup;
        ->raw_failed:;
        adrp x1,extern 0x8000;
        add x1,x1,#0x40;
        add x0,sp,#8;
        bl extern 0xa020;
        add x1,sp,#8;
        mov x0,x19;
        bl extern 0xa030;
        ->cleanup:;
        ldrsb w8,[sp,#0x1f];
        tbz w8,#0x1f,->success;
        add x8,sp,#8;
        add x0,x8,#0x18;
        ldr x1,[sp,#8];
        ldr x8,[sp,#0x18];
        and x2,x8,#0x7fffffffffffffff;
        bl extern 0xa040;
        ldp x29,x30,[sp,#0x40];
        ldp x20,x19,[sp,#0x30];
        add sp,sp,#0x50;
        ret;
        mov x19,x0;
        ldrsb w8,[sp,#0x1f];
        tbz w8,#0x1f,->resume;
        add x0,sp,#8;
        bl extern 0xa050;
        mov x0,x19;
        bl extern 0xa070;
        mov x19,x0;
        ldrsb w8,[sp,#0x1f];
        tbz w8,#0x1f,->resume;
        add x0,sp,#8;
        bl extern 0xa060;
        ->resume:;
        mov x0,x19;
        bl extern 0xa070
    );
    let (token, mut names) = if binary {
        binary_rows()
    } else {
        decimal_rows()
    };
    names.extend(super::names("%lli"));
    for (address, name) in [
        (0x8040, "\"Malformed token\""),
        (0xa000, "token_conversion"),
        (0xa010, "raw_conversion"),
        (0xa020, "diagnostic_string"),
        (0xa030, "report_malformed"),
        (0xa040, "deallocate"),
        (0xa050, "cold_one"),
        (0xa060, "cold_two"),
        (0xa070, "unwind"),
    ] {
        names.insert(address, name.into());
    }
    ReaderInput {
        wrapper: decode_arm64(&wrapper, 0x4000).unwrap(),
        token,
        raw_token: scan_rows(),
        names,
        reader_token_offset: 0x278,
        token_text_offset: 0x10,
    }
}

#[test]
fn both_fixed_wrappers_prove_unscaled_raw_storage_but_not_mode_selection() {
    for (binary, scale) in [(false, 100000), (true, 32768)] {
        let input = fixed_reader(binary);
        let reader = analyze_reader(&input);
        let GrammarProperty::Partial(Some(conversion)) = reader.conversion else {
            panic!("unproved wrapper: {:?}", reader.gaps);
        };
        assert_eq!(conversion.scale, Known(Some(scale)));

        assert_eq!(conversion.accepted_range, fixed_point_range(scale));
        assert_eq!(
            reader.gaps.iter().map(|gap| gap.reason).collect::<Vec<_>>(),
            [
                "numeric-overflow",
                "numeric-lexical-boundary",
                "numeric-trailing-text",
                "numeric-external-library-conversion",
                "numeric-raw-value-mode"
            ]
        );
    }
}

#[test]
fn changed_raw_store_or_selector_cannot_inherit_wrapper_facts() {
    for (index, operation, operands) in [
        (9, "bl", "#0xa080"),
        (10, "cbnz", "w0,#0x4058"),
        (14, "bl", "#0xa080"),
        (17, "str", "x8,[x19]"),
    ] {
        let mut input = fixed_reader(false);
        input.wrapper[index].operation = operation.into();
        input.wrapper[index].operands = operands.into();
        let reader = analyze_reader(&input);
        assert_eq!(reader.conversion, GrammarProperty::Unresolved);
        assert_eq!(reader.gaps[0].reason, "numeric-wrapper-shape");
    }
    let mut input = fixed_reader(true);
    input.raw_token.clear();
    let reader = analyze_reader(&input);
    let GrammarProperty::Partial(Some(conversion)) = reader.conversion else {
        panic!("ordinary path was lost");
    };
    assert_eq!(conversion.width_bits, GrammarProperty::Partial(64));

    assert_eq!(conversion.accepted_range, Unresolved);
    assert!(
        reader
            .gaps
            .iter()
            .any(|gap| gap.reason == "numeric-raw-conversion")
    );
}

#[test]
fn scanner_pointer_and_return_check_are_required_without_proving_the_lexer() {
    let token = TokenInput {
        body: scan_rows(),
        names: names("%i"),
        token_text_offset: 0x10,
    };
    let reader = analyze_token(&token);
    assert!(
        reader
            .gaps
            .iter()
            .any(|gap| gap.reason == "numeric-lexical-boundary")
    );
    assert!(
        reader
            .gaps
            .iter()
            .any(|gap| gap.reason == "numeric-trailing-text")
    );
    for (index, operation, operands) in [
        (3, "ldr", "x0,[x0,#0x18]"),
        (8, "cmp", "w0,#0x1"),
        (9, "cset", "w0,gt"),
    ] {
        let mut changed = token.body.clone();
        changed[index].operation = operation.into();
        changed[index].operands = operands.into();
        assert!(token_conversion(&changed, &token.names, 0x10).is_none());
    }
}

#[test]
fn binary32_bounds_have_a_specific_gap_only_after_the_conversion_is_proved() {
    for (format, expected) in [("%f", true), ("%i", false)] {
        let token = TokenInput {
            body: scan_rows(),
            names: names(format),
            token_text_offset: 0x10,
        };
        let reader = analyze_token(&token);
        assert_eq!(
            reader
                .gaps
                .iter()
                .any(|gap| gap.reason == FLOAT_BOUND_REPRESENTATION_GAP),
            expected
        );
        let GrammarProperty::Partial(Some(conversion)) = reader.conversion else {
            panic!("scanner conversion was not proved");
        };
        assert_eq!(
            conversion.accepted_range,
            if format == "%i" {
                signed_32_range()
            } else {
                Unresolved
            }
        );
    }
    let missing = analyze_token(&TokenInput {
        body: scan_rows(),
        names: BTreeMap::new(),
        token_text_offset: 0x10,
    });
    assert_eq!(missing.conversion, Unresolved);
    assert_eq!(missing.gaps.len(), 1);
    assert_eq!(missing.gaps[0].reason, "numeric-token-shape");
}

#[test]
#[ignore = "requires exact M451-hotfix through STELLARIS_PATH"]
fn m451_numeric_boundary_engine_parity() {
    use crate::binding::inspect::{Image, read_image};
    let path = std::path::PathBuf::from(std::env::var_os("STELLARIS_PATH").unwrap());
    let bytes = read_image(&path).unwrap();
    let image = Image::read(&bytes).unwrap();
    assert_eq!(
        image.identity().unwrap().executable,
        "29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38"
    );
    for (vtable, target, result) in [
        ("vtable for CTextLexer", "CTextLexer::IsBinary() const", 0),
        ("vtable for CBinLexer", "CBinLexer::IsBinary() const", 1),
    ] {
        let address = image.address(target).unwrap();
        let slot = image.address(vtable).unwrap() + 0x10 + 0x20;
        assert_eq!(
            image.slots(slot, 1).unwrap()[0].holds,
            format!("{address:#x} {target}")
        );
        assert_eq!(
            image.lookup_lines(address).unwrap(),
            [format!("mov w0,#{result:#x}"), "ret".into()]
        );
    }
    for (address, operation, operands) in [
        (0x1025b1ddc, "str", "x1,[x0,#0x30]"),
        (0x1025bd798, "mov", "w8,#0x167"),
        (0x1025bd79c, "str", "w8,[x0]"),
        (0x1025bd7c0, "ldr", "x9,[x1]"),
        (0x1025bd154, "strb", "wzr,[x21,x20]"),
    ] {
        let listing = image.disassemble(address, 4).unwrap();
        assert_eq!(listing.rows[0].operation, operation);
        assert_eq!(listing.rows[0].operands, operands);
    }
    let native = crate::Native::open(path).unwrap();
    let facts = crate::internals::numeric_readers::run(&native).unwrap();
    assert_eq!(facts.readers.len(), 11);
    for (name, reader) in facts.readers {
        let GrammarProperty::Partial(Some(conversion)) = reader.conversion else {
            panic!("{name}: {:?}", reader.gaps);
        };
        let range = match name.as_str() {
            "CReader::Read(int&)" => signed_32_range(),
            "CReader::Read(CFixedPoint&)" => fixed_point_range(100000),
            "CReader::Read(fpml::fixed_point<long long, (unsigned char)48, (unsigned char)15>&)" => {
                fixed_point_range(32768)
            }
            _ => Unresolved,
        };
        assert_eq!(conversion.accepted_range, range, "{name}");
        for reason in [
            "numeric-overflow",
            "numeric-lexical-boundary",
            "numeric-trailing-text",
            "numeric-external-library-conversion",
        ] {
            assert!(reader.gaps.iter().any(|gap| gap.reason == reason), "{name}");
        }
        if matches!(conversion.width_bits, Known(8 | 16)) {
            assert_eq!(conversion.signedness, Unresolved, "{name}");
        }
        assert_eq!(
            reader
                .gaps
                .iter()
                .any(|gap| gap.reason == FLOAT_BOUND_REPRESENTATION_GAP),
            conversion.representation == Known(NumericRepresentation::BinaryFloat),
            "{name}"
        );
    }
    let answer = native.registry_fields("common/star_classes").unwrap();
    assert!(answer.gaps.iter().any(|gap| {
        gap.kind == crate::GapKind::NumericConversion
            && gap.subject == Some(crate::GapSubject::field("icon_scale"))
            && gap.detail == "Exact binary32 range endpoints cannot be represented by NumericBound."
    }));
}
