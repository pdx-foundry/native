use super::*;
use crate::engine::analysis::{
    assembler::{Arm64, arm64},
    decode::decode_arm64,
};

const READ_STRING: u64 = 0xa000;
const INSERT: u64 = 0xa100;
const READ_PERSISTENT: u64 = 0xa200;
const BASE_MEMBER: u64 = 0xa300;
const CSTRING: u64 = 0xa400;
const REPORT_UNEXPECTED: u64 = 0xa500;
const LINE_NUMBER: u64 = 0xa600;
const SIMPLE_STATEMENT: u64 = 0xa700;
const READ_VALUE: u64 = 0xa800;
const ASSIGN: u64 = 0xa900;
const REPORT_MALFORMED: u64 = 0xaa00;
const LIST_START: u64 = 0x9000;
const UNEXPECTED_VALUE: u64 = 0x9040;

fn names() -> BTreeMap<u64, String> {
    [
        (READ_STRING, "CReader::Read(CString&, bool)"),
        (
            INSERT,
            "CCustomDescription::SParam& CPdxArray<CCustomDescription::SParam, int>::InsertAtEmplace<>(int, &&)",
        ),
        (READ_PERSISTENT, "CReader::Read(CPersistent&)"),
        (BASE_MEMBER, "CPersistent::ReadMember(CReader&, int)"),
        (CSTRING, "CString::CString(char const*)"),
        (REPORT_UNEXPECTED, "CReader::ReportUnexpected(CString const&)"),
        (LINE_NUMBER, "CReader::GetLineNumber() const"),
        (SIMPLE_STATEMENT, "CReader::ReadSimpleStatement()"),
        (READ_VALUE, "CToken::ReadValue(CString&) const"),
        (ASSIGN, "CToken::operator=(CToken const&)"),
        (REPORT_MALFORMED, "CReader::ReportMalformed(CString const&)"),
        (LIST_START, "\"Expected list start\""),
        (
            UNEXPECTED_VALUE,
            "\"Unexpected value type for custom description modifier parameter\"",
        ),
    ]
    .into_iter()
    .map(|(address, name)| (address, name.to_owned()))
    .collect()
}

fn tokens() -> BTreeMap<i64, Token> {
    [(0x167, "long_float"), (0x414, "num64"), (0x29c, "unum64")]
        .into_iter()
        .map(|(token, name)| {
            let token_name = Token {
                name: Some(name.into()),
                constructor: 0,
            };
            (token, token_name)
        })
        .collect()
}

/// A serializer member at 0x1000 whose list route requires the value token kind `block`.
fn member(block: u32) -> Vec<Instruction> {
    let mut code = Arm64::at(0x1000);
    arm64!(code;
        sub sp, sp, #0x180;
        stp x28, x27, [sp, #0x130];
        stp x24, x23, [sp, #0x140];
        stp x22, x21, [sp, #0x150];
        stp x20, x19, [sp, #0x160];
        stp x29, x30, [sp, #0x170];
        add x29, sp, #0x170;
        mov x19, x1;
        mov x20, x0;
        mov w8, #0x39a4; // token 0x39a4, the list key
        cmp w2, w8;
        b.eq >list;
        mov w8, #0x317a; // token 0x317a, the text key
        cmp w2, w8;
        b.ne >base;
        ldr x1, [x20, #8];
        mov x0, x19;
        mov w2, #0;
        ldp x29, x30, [sp, #0x170];
        ldp x20, x19, [sp, #0x160];
        ldp x22, x21, [sp, #0x150];
        ldp x24, x23, [sp, #0x140];
        ldp x28, x27, [sp, #0x130];
        add sp, sp, #0x180;
        b extern READ_STRING as usize;
        list:;
        ldr w8, [x19, #0x278];
        cmp w8, #block;
        b.ne >error;
        mov w22, #0x13;
        mov w23, #1;
        adrp x24, extern 0x8000;
        add x24, x24, #0x10; // the entry serializer's vtable address point
        ldr x21, [x19, #0x30];
        ldrb w8, [x21, #0x13c];
        cbnz w8, >peeked;
        lex:;
        ldr x8, [x21];
        ldr x8, [x8, #0x10];
        mov x0, x21;
        blr x8;
        str w0, [x21, #0x10];
        cbnz w0, >kept;
        str w22, [x21, #0x18];
        kept:;
        strb w23, [x21, #0x13c];
        peeked:;
        ldr w8, [x21, #0x18];
        cmp w8, #4;
        ccmp w8, #0x13, #4, ne;
        b.eq >done;
        ldr x8, [x20, #8];
        add x0, x8, #0x28;
        ldr w1, [x8, #0x3c];
        bl extern INSERT as usize;
        stp x24, x0, [sp];
        mov x1, sp;
        mov x0, x19;
        bl extern READ_PERSISTENT as usize;
        ldr x21, [x19, #0x30];
        ldrb w8, [x21, #0x13c];
        cbnz w8, <peeked;
        b <lex;
        base:;
        mov x0, x20;
        mov x1, x19;
        ldp x29, x30, [sp, #0x170];
        ldp x20, x19, [sp, #0x160];
        ldp x22, x21, [sp, #0x150];
        ldp x24, x23, [sp, #0x140];
        ldp x28, x27, [sp, #0x130];
        add sp, sp, #0x180;
        b extern BASE_MEMBER as usize;
        error:;
        adrp x1, extern 0x9000;
        add x1, x1, #0;
        mov x0, sp;
        bl extern CSTRING as usize;
        mov x1, sp;
        mov x0, x19;
        bl extern REPORT_UNEXPECTED as usize;
        nop; nop; nop; nop; nop; nop; nop; nop; nop;
        done:;
        ret
    );

    decode_arm64(&code.bytes(), 0x1000).unwrap()
}

/// A parameter entry reader at 0x2000 that accepts the kinds up to 0x14 in both masks, and the
/// keyword kinds 0x167, 0x414 and 0x29c.
fn entry(low_mask: u32) -> Vec<Instruction> {
    let mut code = Arm64::at(0x2000);
    arm64!(code;
        stp x28, x27, [sp, #-0x50]!;
        stp x24, x23, [sp, #0x10];
        stp x22, x21, [sp, #0x20];
        stp x20, x19, [sp, #0x30];
        stp x29, x30, [sp, #0x40];
        add x29, sp, #0x40;
        sub sp, sp, #0x220;
        mov x19, x1;
        mov x20, x0;
        mov x0, x1;
        bl extern LINE_NUMBER as usize;
        mov x21, x0;
        mov x0, x19;
        bl extern SIMPLE_STATEMENT as usize;
        add x0, x19, #0x38;
        ldr x1, [x20, #8];
        bl extern READ_VALUE as usize;
        ldr w8, [x19, #0x278];
        cmp w8, #0x166;
        b.gt >keywords;
        cmp w8, #0x14;
        mov w9, #1;
        lsl w8, w9, w8;
        and w8, w8, #low_mask;
        and w8, w8, #0xfff0ffff;
        ccmp w8, #0, #4, ls;
        b.eq >malformed;
        accept:;
        add x1, x19, #0x278;
        ldr x8, [x20, #8];
        add x0, x8, #0x28;
        bl extern ASSIGN as usize;
        ret;
        keywords:;
        cmp w8, #0x167; // keyword long_float
        b.eq <accept;
        cmp w8, #0x414; // keyword num64
        b.eq <accept;
        cmp w8, #0x29c; // keyword unum64
        b.eq <accept;
        malformed:;
        adrp x1, extern 0x9000;
        add x1, x1, #0x40;
        add x0, sp, #0x100;
        bl extern CSTRING as usize;
        add x1, sp, #0x100;
        mov x0, x19;
        bl extern REPORT_MALFORMED as usize;
        ret
    );

    decode_arm64(&code.bytes(), 0x2000).unwrap()
}

#[test]
fn the_list_shape_gives_the_key_token_the_route_and_the_entry_vtable() {
    let route = list_route(&member(3), &names(), 8, 0x278).unwrap();

    assert_eq!(
        route,
        ListRoute {
            token: 0x39a4,
            branch: 0x1000 + 25 * 4,
            entry_point: 0x8010,
        }
    );
}

#[test]
fn a_list_route_that_does_not_require_a_block_start_fails_closed() {
    let stop = list_route(&member(4), &names(), 8, 0x278).unwrap_err();

    assert_eq!(stop.reason, "parameter-list-shape");
    assert!(list_route(&member(3), &names(), 0x10, 0x278).is_err());
}

#[test]
fn the_entry_shape_gives_the_masked_kinds_and_the_named_keywords() {
    let accepted = accepted_kinds(&entry(0x1ff000), &names(), &tokens(), 0x278).unwrap();

    assert_eq!(accepted.scalar, BTreeSet::from([0xc, 0xd, 0xe, 0xf, 0x14]));
    assert_eq!(
        value_forms(&accepted),
        [
            ScalarForm::Number,
            ScalarForm::Text,
            ScalarForm::Keyword("long_float".into()),
            ScalarForm::Keyword("num64".into()),
            ScalarForm::Keyword("unum64".into()),
        ]
    );
}

#[test]
fn an_entry_with_another_report_fails_closed() {
    let mut names = names();
    names.insert(
        REPORT_MALFORMED,
        "CReader::ReportUnexpected(CString const&)".into(),
    );

    let stop = accepted_kinds(&entry(0x1ff000), &names, &tokens(), 0x278).unwrap_err();

    assert_eq!(stop.reason, "parameter-entry-shape");
}

#[test]
fn a_mask_that_accepts_a_kind_with_no_written_form_fails_closed() {
    let stop = accepted_kinds(&entry(0x1fff8), &names(), &tokens(), 0x278).unwrap_err();

    assert_eq!(stop.reason, "parameter-entry-kind");
}

#[test]
fn a_mask_without_the_number_kind_gives_no_number_form() {
    let accepted = accepted_kinds(&entry(0x1fe000), &names(), &tokens(), 0x278).unwrap();

    assert!(!value_forms(&accepted).contains(&ScalarForm::Number));
    assert!(value_forms(&accepted).contains(&ScalarForm::Text));
}

#[test]
fn an_unnamed_keyword_kind_fails_closed() {
    let mut tokens = tokens();
    tokens.remove(&0x414);

    let stop = accepted_kinds(&entry(0x1ff000), &names(), &tokens, 0x278).unwrap_err();

    assert_eq!(stop.reason, "parameter-entry-kind");
}

fn leaf(domain: i64, instructions: Vec<u64>, terminal: u64, outcome: PathOutcome) -> TokenPath {
    TokenPath {
        domain: [domain, domain],
        conditions: vec![],
        instructions,
        terminal,
        outcome,
    }
}

fn stopped(reason: &'static str) -> PathOutcome {
    PathOutcome::Reader(ReaderJoin::Missing(Unresolved::new(reason)))
}

fn list() -> List {
    List {
        token: 7,
        branch: 0x1010,
        reader: "CSerializer::ReadMember(CReader&, int)".into(),
        accepted: AcceptedKinds {
            scalar: BTreeSet::from([0xc]),
            keywords: BTreeSet::new(),
        },
    }
}

#[test]
fn stopped_leaves_on_the_list_route_join_once_and_other_leaves_keep_their_outcome() {
    let text = PathOutcome::Reader(ReaderJoin::Joined {
        callee: "CReader::Read(CString&, bool)".into(),
        arguments: Default::default(),
        tail: true,
    });
    let mut leaves = vec![
        leaf(7, vec![0x1000, 0x1010], 0x1020, stopped("instruction")),
        leaf(7, vec![0x1000, 0x1010], 0x1030, stopped("reader-routing")),
        leaf(7, vec![0x1000], 0x1008, stopped("instruction")),
        leaf(8, vec![0x1000], 0x1004, text.clone()),
    ];
    let lists = [ParameterList {
        body: 0x1000..0x1100,
        list: Ok(list()),
    }];

    let joined = join(&mut leaves, &lists);

    assert_eq!(joined.keys().copied().collect::<Vec<_>>(), [7]);
    assert_eq!(leaves.len(), 3);
    assert_eq!(
        leaves[0].outcome,
        PathOutcome::Reader(ReaderJoin::Stored {
            callee: "CSerializer::ReadMember(CReader&, int)".into(),
            kind: ReaderKind::Block,
            destination: None,
            repeat: RepeatBehavior::Accumulate,
        })
    );
    assert_eq!(leaves[1].outcome, stopped("instruction"));
    assert_eq!(leaves[2].outcome, text);
}

#[test]
fn a_leaf_that_stopped_in_a_member_with_no_list_takes_its_stop() {
    let mut leaves = vec![
        leaf(7, vec![0x1000, 0x1010], 0x1020, stopped("instruction")),
        leaf(9, vec![0x3000], 0x3010, stopped("instruction")),
    ];
    let lists = [ParameterList {
        body: 0x1000..0x1100,
        list: Err(Unresolved::new("parameter-list-shape")),
    }];

    let joined = join(&mut leaves, &lists);

    assert!(joined.is_empty());
    assert_eq!(leaves[0].outcome, stopped("parameter-list-shape"));
    assert_eq!(leaves[1].outcome, stopped("instruction"));
}
