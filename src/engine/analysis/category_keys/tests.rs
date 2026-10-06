use super::*;
use crate::engine::analysis::assembler::arm64;
use crate::engine::analysis::decode::decode_arm64;

const SWITCH: u64 = 0x1000;
const SITE: u64 = 0x2000;
const LOG: u64 = 0x3000;
const OTHER: u64 = 0x3100;

/// Token 7 is `ship` (0x407c) and token 9 is `all`; every other token gives 0.
fn switch() -> Vec<u8> {
    arm64!(at SWITCH;
        cmp w0, #7; // ship
        b.ne >all;
        mov w0, #0x407c;
        ret;
        all:;
        cmp w0, #9; // all
        b.ne >invalid;
        movn w0, #0;
        ret;
        invalid:;
        mov w0, #0;
        ret
    )
}

/// The ways that a reader's call site can differ from the readers' exact shape.
#[derive(Clone, Copy, PartialEq)]
enum Site {
    Exact,
    BranchOnNotEqual,
    EqualGoesElsewhere,
    ComparesAnotherWord,
    NextCallIsNotTheLog,
    BranchBeforeTheLog,
    SuccessBeforeTheLog,
}

/// A reader that accepts `empty` with an empty mask.
fn site(shape: Site, empty: u32) -> Vec<Instruction> {
    let log = if shape == Site::NextCallIsNotTheLog {
        OTHER
    } else {
        LOG
    };
    let bytes = match shape {
        Site::Exact | Site::NextCallIsNotTheLog => arm64!(at SITE;
            ldr w0, [x20, #0x278];
            bl extern SWITCH as usize;
            str w0, [x19, #0x74];
            cbnz w0, >done;
            ldr w8, [x20, #0x278];
            cmp w8, #empty;
            b.eq >done;
            mov w2, #0x1c;
            bl extern log as usize;
            done:;
            ret
        ),
        Site::BranchOnNotEqual => arm64!(at SITE;
            ldr w0, [x20, #0x278];
            bl extern SWITCH as usize;
            str w0, [x19, #0x74];
            cbnz w0, >done;
            ldr w8, [x20, #0x278];
            cmp w8, #empty;
            b.ne >done;
            bl extern LOG as usize;
            done:;
            ret
        ),
        Site::EqualGoesElsewhere => arm64!(at SITE;
            ldr w0, [x20, #0x278];
            bl extern SWITCH as usize;
            str w0, [x19, #0x74];
            cbnz w0, >done;
            ldr w8, [x20, #0x278];
            cmp w8, #empty;
            b.eq >elsewhere;
            bl extern LOG as usize;
            elsewhere:;
            nop;
            done:;
            ret
        ),
        Site::ComparesAnotherWord => arm64!(at SITE;
            ldr w0, [x20, #0x278];
            bl extern SWITCH as usize;
            str w0, [x19, #0x74];
            cbnz w0, >done;
            ldr w8, [x20, #0x27c];
            cmp w8, #empty;
            b.eq >done;
            bl extern LOG as usize;
            done:;
            ret
        ),
        Site::BranchBeforeTheLog => arm64!(at SITE;
            ldr w0, [x20, #0x278];
            bl extern SWITCH as usize;
            str w0, [x19, #0x74];
            cbnz w0, >done;
            ldr w8, [x20, #0x278];
            cmp w8, #empty;
            b.eq >done;
            cbz w9, >done;
            bl extern LOG as usize;
            done:;
            ret
        ),
        Site::SuccessBeforeTheLog => arm64!(at SITE;
            done:;
            ldr w0, [x20, #0x278];
            bl extern SWITCH as usize;
            str w0, [x19, #0x74];
            cbnz w0, <done;
            ldr w8, [x20, #0x278];
            cmp w8, #empty;
            b.eq <done;
            bl extern LOG as usize;
            ret
        ),
    };

    decode_arm64(&bytes, SITE).unwrap()
}

fn input(sites: Vec<Vec<Instruction>>) -> CategoryKeyInput {
    let bytes = switch();

    CategoryKeyInput {
        tokens: BTreeMap::from([
            (5, "none".into()),
            (6, "pop_job".into()),
            (7, "ship".into()),
            (9, "all".into()),
        ]),
        switch: SWITCH,
        code: Code::decode(&[(SWITCH, bytes.as_slice())]).unwrap(),
        sites,
        log: BTreeSet::from([LOG]),
        categories: CategoryInput {
            category_name: 0,
            string_object_size: 24,
            short_length_offset: 0x17,
            assign_literal: BTreeSet::new(),
            code: Code::default(),
            data: ReadOnlyData::default(),
        },
    }
}

fn keys(result: &CategoryKeyResult) -> Vec<(&str, u64)> {
    result
        .keys
        .iter()
        .map(|(name, &mask)| (name.as_str(), mask))
        .collect()
}

#[test]
fn the_switch_gives_each_key_and_the_readers_give_the_empty_key() {
    let result = analyze(&input(vec![site(Site::Exact, 5), site(Site::Exact, 5)]));

    assert_eq!(
        keys(&result),
        [("all", EVERY_CATEGORY), ("none", 0), ("ship", 0x407c)]
    );
    assert!(result.empty.is_none());
    assert_eq!(result.unreadable, 0);
}

#[test]
fn a_site_that_differs_from_the_readers_shape_gives_no_empty_key() {
    for shape in [
        Site::BranchOnNotEqual,
        Site::EqualGoesElsewhere,
        Site::ComparesAnotherWord,
        Site::NextCallIsNotTheLog,
        Site::BranchBeforeTheLog,
        Site::SuccessBeforeTheLog,
    ] {
        let result = analyze(&input(vec![site(Site::Exact, 5), site(shape, 5)]));

        assert_eq!(keys(&result), [("all", EVERY_CATEGORY), ("ship", 0x407c)]);
        assert_eq!(result.empty.unwrap().reason, "empty-key-shape");
    }
}

#[test]
fn sites_that_accept_different_tokens_give_no_empty_key() {
    let result = analyze(&input(vec![site(Site::Exact, 5), site(Site::Exact, 6)]));

    assert!(!result.keys.contains_key("none"));
    assert_eq!(result.empty.unwrap().reason, "empty-key-sites");
}
