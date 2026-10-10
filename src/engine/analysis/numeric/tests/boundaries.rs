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

fn reasons(gaps: &[super::super::Unresolved]) -> Vec<&'static str> {
    gaps.iter().map(|gap| gap.reason).collect()
}

fn selector(binary: bool) -> LexerInput {
    let rows = if binary {
        arm64!(at 0x5000; mov w0,#1; ret)
    } else {
        arm64!(at 0x5000; mov w0,#0; ret)
    };
    LexerInput {
        body: Vec::new(),
        names: BTreeMap::new(),
        data: Default::default(),
        binary_selector: decode_arm64(&rows, 0x5000).unwrap(),
    }
}

#[test]
fn both_fixed_wrappers_report_the_raw_path_as_binary_input_only_after_the_text_selector() {
    let text = super::super::lexer::text_selector(&selector(false));
    let binary = super::super::lexer::text_selector(&selector(true));
    assert_eq!(text, Ok(()));
    assert_eq!(
        binary.clone().map_err(|obstacle| obstacle.reason),
        Err("numeric-raw-value-mode")
    );

    for (template, scale) in [(false, 100000), (true, 32768)] {
        let input = fixed_reader(template);
        let reader = analyze_reader(&input, &Ok(()), &text);
        let GrammarProperty::Partial(Some(conversion)) = reader.conversion else {
            panic!("unproved wrapper: {:?}", reader.gaps);
        };
        assert_eq!(conversion.scale, Known(Some(scale)));
        assert_eq!(conversion.accepted_range, fixed_point_range(scale));
        assert!(reader.gaps.is_empty(), "{:?}", reader.gaps);
        assert_eq!(
            reasons(&reader.boundary),
            [
                "numeric-overflow",
                "numeric-trailing-text",
                "numeric-external-library-conversion",
                BINARY_INPUT_BOUNDARY
            ]
        );

        let reader = analyze_reader(&input, &Ok(()), &binary);
        assert_eq!(reasons(&reader.gaps), ["numeric-raw-value-mode"]);
        assert!(!reasons(&reader.boundary).contains(&BINARY_INPUT_BOUNDARY));
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
        let reader = analyze_reader(&input, &Ok(()), &Ok(()));
        assert_eq!(reader.conversion, GrammarProperty::Unresolved);
        assert_eq!(reader.gaps[0].reason, "numeric-wrapper-shape");
    }
    let mut input = fixed_reader(true);
    input.raw_token.clear();
    let reader = analyze_reader(&input, &Ok(()), &Ok(()));
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
fn a_lexer_obstacle_is_the_token_text_gap_of_readers_and_token_readers() {
    let obstacle = Err(super::super::Unresolved::new("numeric-lexer-shape"));
    let reader = analyze_reader(&fixed_reader(false), &obstacle, &Ok(()));
    let token = analyze_token(
        &TokenInput {
            body: scan_rows(),
            names: names("%i"),
            token_text_offset: 0x10,
        },
        &obstacle,
    );
    for gaps in [&reader.gaps, &token.gaps] {
        assert_eq!(gaps[0].reason, "numeric-lexer-shape");
        assert!(
            gaps.iter()
                .all(|gap| gap.reason != "numeric-lexical-boundary")
        );
    }
    assert!(matches!(
        reader.conversion,
        GrammarProperty::Partial(Some(_))
    ));
}

#[test]
fn scanner_pointer_and_return_check_are_required() {
    let token = TokenInput {
        body: scan_rows(),
        names: names("%i"),
        token_text_offset: 0x10,
    };
    let reader = analyze_token(&token, &Ok(()));
    assert!(reader.gaps.is_empty());
    assert!(reasons(&reader.boundary).contains(&"numeric-trailing-text"));
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
        let reader = analyze_token(&token, &Ok(()));
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
    let missing = analyze_token(
        &TokenInput {
            body: scan_rows(),
            names: BTreeMap::new(),
            token_text_offset: 0x10,
        },
        &Ok(()),
    );
    assert_eq!(missing.conversion, Unresolved);
    assert_eq!(missing.gaps.len(), 1);
    assert_eq!(missing.gaps[0].reason, "numeric-token-shape");
}

#[test]
#[ignore = "requires exact M452 through STELLARIS_PATH"]
fn m452_numeric_boundary_engine_parity() {
    use crate::binding::inspect::{Image, read_image};
    let path = std::path::PathBuf::from(std::env::var_os("STELLARIS_PATH").unwrap());
    let bytes = read_image(&path).unwrap();
    let image = Image::read(&bytes).unwrap();
    assert_eq!(
        image.identity().unwrap().executable,
        "c621723d9c8e0c1cd153319208d30a9dfbb9e63675be86f9d0ae7debeaa7fe1b"
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
    text_lexer_census(&image);
    // The lexer's input is the `CFile` that both `CTextLexer(CFile*, bool)` bodies store at
    // `+8`; GetTok calls its slots `0x10` and `0x58`.
    for class in ["CMemoryFile", "CArchiveFile"] {
        let vtable = image.address(&format!("vtable for {class}")).unwrap() + 0x10;
        for (slot, member) in [(0x10, "Get()"), (0x58, "IsValid() const")] {
            let target = format!("{class}::{member}");
            let address = image.address(&target).unwrap();
            assert_eq!(
                image.slots(vtable + slot, 1).unwrap()[0].holds,
                format!("{address:#x} {target}")
            );
        }
    }
    for (address, operation, operands) in [
        (0x1025b0e88, "stp", "x8,x1,[x0],#0x18"),
        (0x1025b0f44, "stp", "x8,x1,[x0],#0x18"),
        (0x1025b4d18, "str", "x1,[x0,#0x30]"),
        (0x1025c06d4, "mov", "w8,#0x167"),
        (0x1025c06d8, "str", "w8,[x0]"),
        (0x1025c06fc, "ldr", "x9,[x1]"),
        (0x1025c0090, "strb", "wzr,[x21,x20]"),
    ] {
        let listing = image.disassemble(address, 4).unwrap();
        assert_eq!(listing.rows[0].operation, operation);
        assert_eq!(listing.rows[0].operands, operands);
    }
    let native = crate::Native::open(path).unwrap();
    let facts = crate::internals::numeric_readers::run(&native).unwrap();
    assert_eq!(facts.readers.len(), 11);
    for (name, reader) in &facts.token_readers {
        assert!(
            reader.gaps.iter().all(
                |gap| !gap.reason.contains("lexer") && gap.reason != "numeric-lexical-boundary"
            ),
            "{name}"
        );
    }
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
        let fixed_point = name.contains("CFixedPoint") || name.contains("fixed_point<");
        let mut boundary = SCANNER_BOUNDARY.to_vec();
        if fixed_point {
            boundary.push(BINARY_INPUT_BOUNDARY);
        }
        assert_eq!(reasons(&reader.boundary), boundary, "{name}");
        let float = conversion.representation == Known(NumericRepresentation::BinaryFloat);
        let typed: &[&str] = if float {
            &[FLOAT_BOUND_REPRESENTATION_GAP]
        } else {
            &[]
        };
        assert_eq!(reasons(&reader.gaps), typed, "{name}");
        if matches!(conversion.width_bits, Known(8 | 16)) {
            assert_eq!(conversion.signedness, Unresolved, "{name}");
        }
    }
    let answer = native.registry_fields("common/star_classes").unwrap();
    assert!(answer.gaps.iter().any(|gap| {
        gap.kind == crate::GapKind::NumericConversion
            && gap.subject == Some(crate::GapSubject::field("icon_scale"))
            && gap.detail == "Exact binary32 range endpoints cannot be represented by NumericBound."
    }));
}

/// The stated input rule of the fixed-point raw path, checked on the whole build: every direct
/// construction of a `CReader` outside save game, network and command packet code passes a
/// `CTextLexer` that its own function constructed, and only that code constructs a `CBinLexer`.
fn text_lexer_census(image: &crate::binding::inspect::Image<'_>) {
    let address = |name: &str| -> Vec<u64> {
        image
            .symbols(name)
            .into_iter()
            .filter(|(_, symbol)| *symbol == name)
            .map(|(address, _)| address)
            .collect()
    };
    let callers = |target: u64| -> Vec<crate::binding::inspect::Caller> {
        image
            .callers(target)
            .into_iter()
            .filter(|caller| caller.operation == "bl")
            .collect()
    };
    let text_constructors: Vec<u64> = [
        "CTextLexer::CTextLexer(CString const&, ELexerFileType)",
        "CTextLexer::CTextLexer(CFile*, bool)",
    ]
    .into_iter()
    .flat_map(address)
    .collect();

    let sites: Vec<_> = [
        "CReader::CReader(CLexer&)",
        "CReader::CReader(CLexer*, bool)",
    ]
    .into_iter()
    .flat_map(address)
    .flat_map(callers)
    .collect();
    let binary_input = |site: &crate::binding::inspect::Caller| {
        image.function_at(site.at).is_some_and(|function| {
            [
                "CreateCommand(",
                "CNetworkServer::",
                "CProxyServer::",
                "SaveGame::",
            ]
            .iter()
            .any(|prefix| function.starts_with(prefix))
        })
    };
    let (binary, unmatched): (Vec<_>, Vec<_>) = sites
        .iter()
        .filter(|site| !passes_its_own_text_lexer(image, site.at, &text_constructors))
        .partition(|site| binary_input(site));
    let places = |sites: &[&crate::binding::inspect::Caller]| -> Vec<String> {
        sites.iter().map(|site| site.place.clone()).collect()
    };
    assert_eq!(sites.len(), 269);
    assert!(unmatched.is_empty(), "{:#?}", places(&unmatched));
    assert_eq!(binary.len(), 8, "{:#?}", places(&binary));

    let mut binary_callers: Vec<_> = [
        "CBinLexer::CBinLexer(CFile*, bool)",
        "CBinLexer::CBinLexer(CString const&)",
    ]
    .into_iter()
    .flat_map(address)
    .flat_map(callers)
    .filter_map(|caller| {
        let function = image.function_at(caller.at)?;
        Some(function.split(['(', ':']).next()?.to_owned())
    })
    .collect();
    binary_callers.sort();
    binary_callers.dedup();
    assert_eq!(
        binary_callers,
        [
            "CNetworkServer",
            "CProxyServer",
            "CreateCommand",
            "SaveGame"
        ]
    );
}

/// Whether the reader constructed at `call` receives, in `x1`, the object that a `CTextLexer`
/// constructor earlier in the same function received in `x0`.
fn passes_its_own_text_lexer(
    image: &crate::binding::inspect::Image<'_>,
    call: u64,
    text_constructors: &[u64],
) -> bool {
    let Some(function) = image.function_at(call) else {
        return false;
    };
    let Some(start) = image
        .symbols(function)
        .into_iter()
        .filter(|(address, symbol)| *symbol == function && *address <= call)
        .map(|(address, _)| address)
        .max()
    else {
        return false;
    };
    let Ok(listing) = image.disassemble(start, call - start + 4) else {
        return false;
    };
    let rows = &listing.rows;
    let Some(reader_call) = rows.iter().position(|row| row.address == call) else {
        return false;
    };
    let Some(lexer) = value_of(&rows[..reader_call], "x1") else {
        return false;
    };

    rows[..reader_call].iter().enumerate().any(|(index, row)| {
        row.operation == "bl"
            && text_constructors
                .iter()
                .any(|target| row.operands == format!("#{target:#x}"))
            && value_of(&rows[..index], "x0").as_deref() == Some(lexer.as_str())
    })
}

/// The value that `register` holds after `rows`, followed through register copies: a stack or
/// frame address, or the result of a call. `None` when another instruction or a call that does
/// not return it last wrote the register.
fn value_of(rows: &[crate::binding::inspect::Row], register: &str) -> Option<String> {
    let number = &register[1..];
    let names = [format!("x{number}"), format!("w{number}")];
    let writes = |row: &crate::binding::inspect::Row| {
        let first = row.operands.split(',').next().unwrap_or_default();
        let stores = ["str", "stp", "stur", "strb", "strh", "cmp", "cmn", "tst"];
        !stores.contains(&row.operation.as_str())
            && !row.operation.starts_with('b')
            && !row.operation.starts_with("cb")
            && !row.operation.starts_with("tb")
            && names.iter().any(|name| name == first)
    };
    let call = |row: &crate::binding::inspect::Row| matches!(row.operation.as_str(), "bl" | "blr");
    let clobbered = number.parse::<u8>().is_ok_and(|number| number <= 17);
    let index = rows
        .iter()
        .rposition(|row| writes(row) || (clobbered && call(row)))?;
    let row = &rows[index];
    if call(row) {
        return (number == "0").then(|| format!("call {:#x}", row.address));
    }
    let source = row.operands.split_once(',')?.1;

    match row.operation.as_str() {
        "mov" if source == "sp" => Some("sp,#0x0".to_owned()),
        "mov" if source.starts_with('x') => value_of(&rows[..index], source),
        "add" | "sub" if source.starts_with("sp,") || source.starts_with("x29,") => {
            Some(format!("{} {source}", row.operation))
        }
        _ => None,
    }
}
