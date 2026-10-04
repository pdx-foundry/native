use super::*;
use crate::engine::analysis::assembler::{Arm64, arm64};

const POINT: u64 = 0x8000;
const MODIFIER_POINT: u64 = 0xb000;
const MODIFIER_MEMBER: &str = "Modifier::ReadMember(CReader&, int)";

/// A member reader with `key` and `modifier` keys. `other_keys` is the four instructions at
/// `0x1010` that every other token reaches.
fn member(other_keys: Vec<u8>) -> Function {
    assert_eq!(other_keys.len(), 16);
    let mut code = arm64!(at 0x1000;
        cmp w2, #7;
        b.eq extern 0x1020;
        cmp w2, #8;
        b.eq extern 0x1030
    );
    code.extend(other_keys);
    code.extend(arm64!(at 0x1020;
        add x8, x0, #0x10;
        mov x0, x1;
        mov x1, x8;
        b extern 0x4000; // string reader
        add x8, x0, #0x238;
        mov x0, x1;
        mov x1, x8;
        b extern 0x4100 // persistent reader
    ));

    Function {
        address: 0x1000,
        code,
    }
}

/// The default route: a tail call through the owner's delegate slot.
fn virtual_tail() -> Vec<u8> {
    arm64!(at 0x1010;
        ldr x8, [x0];
        ldr x8, [x8, #0x40]; // delegate slot
        br x8;
        nop
    )
}

fn delegate(code: Vec<u8>) -> Function {
    Function {
        address: 0x2000,
        code,
    }
}

fn forwarding_delegate() -> Function {
    delegate(arm64!(at 0x2000;
        add x0, x0, #0x238;
        b extern 0x3000
    ))
}

fn constructor(address: u64, embedded: bool) -> fields::Function {
    let mut code = Arm64::at(address);
    code.prologue();
    if embedded {
        arm64!(code; mov x19, x0; add x0, x0, #0x238; bl extern 0xa000; mov x0, x19);
    } else {
        arm64!(code; str xzr, [x0, #0x238]);
    }
    code.epilogue();
    arm64!(code; ret);

    fields::Function {
        name: "Clause::Clause()".into(),
        address,
        code: code.bytes(),
    }
}

fn class(member: &str) -> PersistentInput {
    PersistentInput {
        constructor_calls: Default::default(),
        constructor_bodies: BTreeMap::new(),
        constructors: vec![constructor(0x9000, true)],
        summaries: BTreeMap::from([(0xa000, BTreeMap::from([(0, MODIFIER_POINT)]))]),
        pointers: BTreeMap::new(),
        writable_slots: Default::default(),
        never_return: vec![],
        requested_words: BTreeMap::new(),
        readers: BTreeMap::from([(
            MODIFIER_POINT,
            ConcreteReader {
                read: "Modifier::Read(CReader&)".into(),
                member: member.into(),
                family: crate::BlockFamily::Modifier,
                delegate: None,
            },
        )]),
    }
}

fn symbol(name: &str, address: u64) -> Symbol {
    Symbol {
        name: name.into(),
        address,
    }
}

fn token(name: &str) -> Token {
    Token {
        name: name.into(),
        constructor: 0,
        ambiguous: false,
    }
}

fn input() -> TriggeredInput {
    TriggeredInput {
        points: BTreeMap::from([(
            POINT,
            ConcreteReader {
                read: "CPersistent::Read(CReader&)".into(),
                member: "Clause::ReadMember(CReader&, int)".into(),
                family: crate::BlockFamily::TriggeredModifier,
                delegate: Some("Clause::ReadModifier(CReader&, int)".into()),
            },
        )]),
        classes: BTreeMap::from([(POINT, Ok(class(MODIFIER_MEMBER)))]),
        functions: BTreeMap::from([
            (0x1000, member(virtual_tail())),
            (0x2000, forwarding_delegate()),
        ]),
        symbols: vec![
            symbol("Clause::ReadMember(CReader&, int)", 0x1000),
            symbol("Clause::ReadModifier(CReader&, int)", 0x2000),
            symbol(MODIFIER_MEMBER, 0x3000),
            symbol("CReader::Read(CString&, bool)", 0x4000),
            symbol("CReader::Read(CPersistent&)", 0x4100),
        ],
        pointers: BTreeMap::from([(POINT + 0x40, 0x2000)]),
        data: vec![],
        tokens: BTreeMap::from([(7, token("key")), (8, token("modifier"))]),
        key_readers: KeyReaders::default(),
        reader_token_offset: 0x38,
    }
}

fn clause(input: &TriggeredInput) -> Clause {
    analyze(input).points.remove(&POINT).unwrap().unwrap()
}

fn other_keys_stop(input: &TriggeredInput) -> &'static str {
    clause(input).other_keys.unwrap_err().reason
}

#[test]
fn the_default_route_and_the_modifier_key_reach_one_embedded_modifier() {
    let clause = clause(&input());
    let names: Vec<_> = clause
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();

    assert_eq!(names, ["key", "modifier"]);
    assert!(clause.fixed_complete);
    assert_eq!(clause.other_keys.unwrap(), 0x238);
    assert_eq!(clause.embedded[&0x238].0, MODIFIER_POINT);
    assert_eq!(
        clause.embedded[&0x238].1.as_ref().unwrap().member,
        MODIFIER_MEMBER
    );
}

#[test]
fn a_delegate_to_another_member_than_the_embedded_object_is_not_joined() {
    let mut input = input();
    input.classes = BTreeMap::from([(POINT, Ok(class("Other::ReadMember(CReader&, int)")))]);

    assert_eq!(other_keys_stop(&input), "triggered-embedded-member");
}

#[test]
fn constructors_that_disagree_on_the_embedded_point_establish_no_join() {
    let mut input = input();
    let mut binding = class(MODIFIER_MEMBER);
    binding.constructors.push(constructor(0x9800, false));
    input.classes = BTreeMap::from([(POINT, Ok(binding))]);
    let clause = clause(&input);

    assert!(clause.embedded.is_empty());
    assert_eq!(
        clause.other_keys.unwrap_err().reason,
        "triggered-embedded-member"
    );
}

#[test]
fn a_delegate_that_calls_before_forwarding_is_a_stated_variant() {
    let mut input = input();
    input.functions.insert(
        0x2000,
        delegate(arm64!(at 0x2000;
            bl extern 0x5000; // lookup
            add x0, x0, #0x238;
            b extern 0x3000
        )),
    );

    assert_eq!(other_keys_stop(&input), "triggered-delegate");
}

#[test]
fn only_a_tail_call_with_the_member_arguments_joins_the_delegate() {
    let non_tail = arm64!(at 0x1010;
        ldr x8, [x0];
        ldr x8, [x8, #0x40];
        blr x8;
        ret
    );
    let wrong_token = arm64!(at 0x1010;
        mov w2, #0;
        ldr x8, [x0];
        ldr x8, [x8, #0x40];
        br x8
    );
    let wrong_slot = arm64!(at 0x1010;
        ldr x8, [x0];
        ldr x8, [x8, #0x48];
        br x8;
        nop
    );

    for other_keys in [non_tail, wrong_token, wrong_slot] {
        let mut input = input();
        input.functions.insert(0x1000, member(other_keys));

        assert_eq!(other_keys_stop(&input), "triggered-default-route");
    }
}

#[test]
fn a_reader_without_a_bound_delegate_reports_the_slot() {
    let mut input = input();
    input.points.get_mut(&POINT).unwrap().delegate = None;

    assert_eq!(other_keys_stop(&input), "triggered-delegate-slot");
}
