use super::*;
use crate::engine::analysis::assembler::{Arm64, arm64};

const MEMBER: u64 = 0x1000;
const FIXED: u64 = 0x2000;
const GET_INT: u64 = 0x2100;
const NEW: u64 = 0x2200;
const INSERT: u64 = 0x2300;
const OPERAND_READ: u64 = 0x2400;
const LOG: u64 = 0x2500;
const FAMILY: u64 = 0x2600;
const ENTRY_READ: u64 = 0x2700;
const PERSISTENT: u64 = 0x2800;
const UNKNOWN: u64 = 0x2900;
const ENUM_SWITCH: u64 = 0x3000;
const ENTRY_MEMBER: u64 = 0x5000;
const MODIFIER: u64 = 0x4000;
const MODIFIER_READ: u64 = 0x4800;
const LOCATION: u64 = 0x3100;
const STRING_MOVE: u64 = 0x3200;
const STRING_ASSIGN: u64 = 0x3300;
const STRING_LENGTH: u64 = 0x3400;
const COPY_TOKEN: u64 = 0x3500;
const MAKE_TARGET: u64 = 0x3600;
const ASSIGN_TARGET: u64 = 0x3700;
const LOOKUP: u64 = 0x3800;
const MALFORMED: u64 = 0x3900;
const READ: u64 = 0x6000;
const VARIANT: u64 = 0x7000;
const VTABLE: u64 = 0x9010;
const POINT: u64 = 0x8000;
const WEIGHT_READ: &str = "Weight::Read(CReader&)";

/// The tokens that the authored readers test, with the literal that names each.
const TOKENS: [(i64, &str); 12] = [
    (7, "base"),
    (8, "days"),
    (9, "modifier"),
    (20, "add"),
    (21, "round"),
    (22, "always"),
    (30, "potential"),
    (40, "scope"),
    (41, "calc"),
    (42, "trigger"),
    (43, "parameters"),
    (44, "mode"),
];

/// How an authored weight member reader departs from the positive shape.
#[derive(Clone, Copy, Default)]
struct Variation {
    /// `base` passes the owner, not the reader, as the reader argument.
    wrong_reader: bool,
    /// The `modifier` entry is read but never inserted.
    no_insert: bool,
    /// `days` calls a function that receives the owner.
    owner_call: bool,
    /// The unnamed token 15 reads `base`.
    interior_key: bool,
    /// `calc` stores the value `always` in a different word from the value `add`.
    split_keyword: bool,
    /// `trigger` copies a text that is not the value token's into the owner.
    other_text: bool,
}

/// How an authored read entry departs from a plain one.
#[derive(Clone, Copy, Default)]
struct ReadVariation {
    /// The entry overwrites the stored scope before it reads.
    clobber: bool,
    /// The entry moves a location string into the owner at this offset first.
    location: Option<u32>,
}

/// A weight member reader: `base`, `days` and a nested `modifier` entry are fixed keys; other
/// keys go through an operation switch, and an unknown key writes an error flag and a
/// diagnostic.
fn member(variation: Variation) -> Vec<u8> {
    let mut code = Arm64::at(MEMBER);
    arm64!(code;
        sub sp, sp, #64;
        mov x19, x0;
        mov x20, x1
    );
    if variation.interior_key {
        arm64!(code; cmp w2, #15; b.eq >base);
    }
    arm64!(code;
        cmp w2, #7; b.eq >base;
        cmp w2, #8; b.eq >days;
        cmp w2, #9; b.eq >modifier;
        ldr w8, [x20, #0x38];
        str w8, [sp, #40];
        add x0, sp, #40;
        bl extern ENUM_SWITCH as usize;
        cmp w0, #16; b.eq >unknown;
        mov w23, w0;
        cmp w23, #3; b.eq >construct;
        mov x0, sp;
        mov x1, x20;
        ldr x2, [x19, #0x30];
        bl extern OPERAND_READ as usize;
        construct:;
        mov w0, #0x40;
        bl extern NEW as usize;
        str w23, [x0, #0x20];
        str x0, [sp, #24];
        add x0, x19, #0x18;
        add x2, sp, #24;
        bl extern INSERT as usize;
        add sp, sp, #64;
        ret;
        unknown:;
        str wzr, [x19, #0x40];
        bl extern LOG as usize;
        add sp, sp, #64;
        ret;
        base:
    );
    if variation.wrong_reader {
        arm64!(code; mov x0, x19);
    } else {
        arm64!(code; mov x0, x20);
    }
    arm64!(code;
        add x1, x19, #0x10;
        add sp, sp, #64;
        b extern FIXED as usize;
        days:
    );
    if variation.owner_call {
        arm64!(code; mov x0, x19; bl extern UNKNOWN as usize);
    }
    arm64!(code;
        add x0, x20, #0x278;
        bl extern GET_INT as usize;
        str x0, [x19, #0x10];
        add sp, sp, #64;
        ret;
        modifier:;
        mov w0, #0x40;
        bl extern NEW as usize;
        mov x21, x0
    );
    code.address(8, VTABLE);
    arm64!(code;
        str x8, [x21];
        ldr x2, [x19, #0x30];
        ldr x8, [x21];
        ldr x8, [x8];
        mov x0, x21;
        mov x1, x20;
        blr x8
    );
    if !variation.no_insert {
        arm64!(code;
            str x21, [sp, #24];
            add x0, x19, #0x18;
            add x2, sp, #24;
            bl extern INSERT as usize
        );
    }
    arm64!(code; add sp, sp, #64; ret);
    code.bytes()
}

/// The entry reader: one operation slot that a repeated operation replaces after a diagnostic,
/// a scoped operand, and other keys delegated to the trigger family in the received scope.
fn entry_member() -> Vec<u8> {
    arm64!(at ENTRY_MEMBER;
        sub sp, sp, #32;
        mov x19, x0;
        mov x20, x1;
        mov w21, w2;
        mov x22, x3;
        ldr w8, [x20, #0x38];
        str w8, [sp, #8];
        add x0, sp, #8;
        bl extern ENUM_SWITCH as usize;
        cmp w0, #16; b.eq >other;
        mov w23, w0;
        ldr w8, [x19, #0x20];
        cmp w8, #16; b.eq >store;
        bl extern LOG as usize;
        store:;
        str w23, [x19, #0x20];
        cmp w23, #3; b.eq >done;
        add x0, x19, #0x28;
        mov x1, x20;
        mov x2, x22;
        add sp, sp, #32;
        b extern OPERAND_READ as usize;
        done:;
        add sp, sp, #32;
        ret;
        other:;
        mov x0, x19;
        mov x1, x20;
        mov w2, w21;
        mov x3, x22;
        add sp, sp, #32;
        b extern FAMILY as usize
    )
}

/// A modifier entry reader in the shape of the scaled and complex weight modifiers: `scope`
/// assigns an event target made from the value token, `calc` compares the value with `add` and
/// `always`, `trigger` looks the value up as a trigger, `parameters` is read by that trigger, and
/// `mode` maps the value through the operation switch. Every other key is malformed.
fn modifier(variation: Variation) -> Vec<u8> {
    let mut code = Arm64::at(MODIFIER);
    arm64!(code;
        sub sp, sp, #0x40;
        mov x19, x0;
        mov x20, x1;
        cmp w2, #40; b.eq >scope;
        cmp w2, #41; b.eq >calc;
        cmp w2, #42; b.eq >trigger;
        cmp w2, #43; b.eq >parameters;
        cmp w2, #44; b.eq >mode;
        add sp, sp, #0x40;
        mov x0, x20;
        b extern MALFORMED as usize;
        scope:;
        add x1, x20, #0x278;
        add x0, sp, #0x10;
        bl extern COPY_TOKEN as usize;
        add x0, sp, #0x20;
        add x1, sp, #0x10;
        bl extern MAKE_TARGET as usize;
        add x0, x19, #0x28;
        add x1, sp, #0x20;
        bl extern ASSIGN_TARGET as usize;
        add sp, sp, #0x40;
        ret;
        calc:;
        ldr w8, [x20, #0x278];
        cmp w8, #20; b.eq >linear;
        cmp w8, #22; b.eq >constant;
        add sp, sp, #0x40;
        mov x0, x20;
        b extern MALFORMED as usize;
        linear:;
        mov w8, #1;
        strb w8, [x19, #0x29a];
        add sp, sp, #0x40;
        ret;
        constant:;
        mov w8, #2
    );
    if variation.split_keyword {
        arm64!(code; strb w8, [x19, #0x2a0]);
    } else {
        arm64!(code; strb w8, [x19, #0x29a]);
    }
    arm64!(code;
        add sp, sp, #0x40;
        ret;
        trigger:
    );
    if variation.other_text {
        arm64!(code; ldr x22, [x20, #0x290]);
    } else {
        arm64!(code; ldr x22, [x20, #0x288]);
    }
    arm64!(code;
        mov x0, x22;
        bl extern STRING_LENGTH as usize;
        mov x2, x0;
        add x0, x19, #0x60;
        mov x1, x22;
        bl extern STRING_ASSIGN as usize;
        mov x0, xzr;
        add x1, x20, #0x278;
        add x2, sp, #0x10;
        bl extern LOOKUP as usize;
        str x0, [x19, #0x88];
        add sp, sp, #0x40;
        ret;
        parameters:;
        ldr x0, [x19, #0x88];
        cbz x0, >missing;
        ldr x8, [x0];
        ldr x3, [x8, #0x30];
        mov x1, x20;
        mov x2, #0;
        add sp, sp, #0x40;
        br x3;
        missing:;
        bl extern LOG as usize;
        add sp, sp, #0x40;
        ret;
        mode:;
        ldr w8, [x20, #0x278];
        str w8, [sp, #8];
        add x0, sp, #8;
        bl extern ENUM_SWITCH as usize;
        str w0, [x19, #0x2e0];
        add sp, sp, #0x40;
        ret
    );
    code.bytes()
}

/// The modifier's read entry: it records its location in the owner, then reads its persistent
/// base as a block.
fn modifier_read() -> Vec<u8> {
    arm64!(at MODIFIER_READ;
        sub sp, sp, #0x20;
        mov x19, x1;
        mov x20, x0;
        add x8, sp, #8;
        mov x0, x1;
        bl extern LOCATION as usize;
        add x0, x20, #0x1b8;
        add x1, sp, #8;
        bl extern STRING_MOVE as usize;
        add x0, x20, #8;
        mov x1, x19;
        add sp, sp, #0x20;
        b extern PERSISTENT as usize
    )
}

/// The operation switch: `add` is 1, `round` is 3, and every other token is 16.
fn switch() -> Vec<u8> {
    arm64!(at ENUM_SWITCH;
        ldr w8, [x0];
        cmp w8, #20; b.eq >add;
        cmp w8, #21; b.eq >round;
        mov w0, #16;
        ret;
        add:;
        mov w0, #1;
        ret;
        round:;
        mov w0, #3;
        ret
    )
}

/// The read entry: a numeric value token sets `base`; anything else is read as a block.
fn read(variation: ReadVariation) -> Vec<u8> {
    let mut code = Arm64::at(READ);
    if variation.clobber {
        arm64!(code; str xzr, [x0, #0x30]);
    }
    if let Some(offset) = variation.location {
        arm64!(code;
            sub sp, sp, #0x30;
            stp x0, x1, [sp, #0x20];
            add x8, sp, #8;
            mov x0, x1;
            bl extern LOCATION as usize;
            ldr x0, [sp, #0x20];
            add x0, x0, #offset;
            add x1, sp, #8;
            bl extern STRING_MOVE as usize;
            ldp x0, x1, [sp, #0x20];
            add sp, sp, #0x30
        );
    }
    arm64!(code;
        ldr w8, [x1, #0x278];
        cmp w8, #12; b.ne >block;
        mov x19, x0;
        mov x0, x1;
        add x1, x19, #0x10;
        b extern FIXED as usize;
        block:;
        b extern PERSISTENT as usize
    );
    code.bytes()
}

/// A variant reader that reads `base` itself and delegates every other key.
fn variant() -> Vec<u8> {
    arm64!(at VARIANT;
        cmp w2, #7; b.ne >inherited;
        mov x8, x0;
        mov x0, x1;
        add x1, x8, #0x38;
        b extern FIXED as usize;
        inherited:;
        b extern MEMBER as usize
    )
}

fn input(read_name: &str, member_name: &str) -> WeightBlockInput {
    let names = [
        (MEMBER, "Weight::ReadMember(CReader&, int)"),
        (FIXED, "CReader::Read(CFixedPoint&)"),
        (GET_INT, "CToken::GetInt() const"),
        (NEW, "operator new(unsigned long)"),
        (
            INSERT,
            "IEntry*& CPdxArray<IEntry*, int>::InsertAtEmplace<IEntry* const&>(int, IEntry* const&)",
        ),
        (OPERAND_READ, "CVariableValue::Read(CReader&, EScopeType)"),
        (LOG, "CLogger::Log(char const*, unsigned int, int)"),
        (
            FAMILY,
            "CTriggerCollectionBase::ReadMember(CReader&, int, EScopeType)",
        ),
        (ENTRY_READ, "Entry::Read(CReader&, EScopeType)"),
        (PERSISTENT, "CPersistent::Read(CReader&)"),
        (UNKNOWN, "Unknown::Call(Owner*)"),
        (
            ENUM_SWITCH,
            "EOperation TokenToEnum<EOperation>(int const&)",
        ),
        (ENTRY_MEMBER, "Entry::ReadMember(CReader&, int, EScopeType)"),
        (READ, "Weight::Read(CReader&)"),
        (VARIANT, "Variant::ReadMember(CReader&, int)"),
        (MODIFIER, "Modifier::ReadMember(CReader&, int)"),
        (MODIFIER_READ, "Modifier::Read(CReader&)"),
        (LOCATION, "CReader::GetFileLocationDescription() const"),
        (
            STRING_MOVE,
            "std::__1::basic_string<char, std::__1::char_traits<char>, CPdxCommonStringAllocator>::__move_assign(std::__1::basic_string<char, std::__1::char_traits<char>, CPdxCommonStringAllocator>&, std::__1::integral_constant<bool, false>)",
        ),
        (
            STRING_ASSIGN,
            "std::__1::basic_string<char, std::__1::char_traits<char>, CPdxCommonStringAllocator>::__assign_external(char const*, unsigned long)",
        ),
        (STRING_LENGTH, "_strlen"),
        (COPY_TOKEN, "CToken::CToken(CToken const&)"),
        (MAKE_TARGET, "CEventTarget::CEventTarget(CToken)"),
        (ASSIGN_TARGET, "CEventTarget::operator=(CEventTarget&&)"),
        (LOOKUP, readers::TRIGGER_LOOKUP),
        (MALFORMED, "CReader::ReportMalformed()"),
    ];
    let pointers = BTreeMap::from([(VTABLE, ENTRY_READ), (VTABLE + 8, ENTRY_MEMBER)]);
    WeightBlockInput {
        points: BTreeMap::from([(
            POINT,
            ConcreteReader {
                read: read_name.into(),
                member: member_name.into(),
                family: crate::BlockFamily::Weight,
            },
        )]),
        constructors: BTreeMap::new(),
        operator_new: BTreeSet::from([NEW]),
        symbols: names
            .iter()
            .map(|&(address, name)| Symbol {
                name: name.into(),
                address,
            })
            .collect(),
        tokens: TOKENS
            .iter()
            .map(|&(token, name)| {
                (
                    token,
                    Token {
                        name: name.into(),
                        constructor: 0,
                        ambiguous: false,
                    },
                )
            })
            .collect(),
        families: BTreeMap::from([(
            "CTriggerCollectionBase::ReadMember(CReader&, int, EScopeType)".into(),
            crate::BlockFamily::Trigger,
        )]),
        data: ReadOnlyData::new(Vec::new()).with_words(&pointers),
        pointers,
        reader_token_offset: 0x38,
        value_token_offset: 0x278,
        token_text_offset: 0x10,
    }
}

fn bodies(variation: Variation, read_variation: ReadVariation) -> BTreeMap<u64, Vec<u8>> {
    BTreeMap::from([
        (MEMBER, member(variation)),
        (ENTRY_MEMBER, entry_member()),
        (ENUM_SWITCH, switch()),
        (READ, read(read_variation)),
        (VARIANT, variant()),
        (MODIFIER, modifier(variation)),
        (MODIFIER_READ, modifier_read()),
    ])
}

fn facts(variation: Variation, member_name: &str, read: ReadVariation) -> WeightBlockFacts {
    let bodies = bodies(variation, read);
    let body = |address: u64| bodies.get(&address).map(Vec::as_slice);
    analyze(&input(WEIGHT_READ, member_name), &body)
}

fn analyze_member(variation: Variation, member_name: &str) -> Grammar {
    facts(variation, member_name, ReadVariation::default()).points[&POINT]
        .clone()
        .unwrap()
}

fn modifier_grammar(variation: Variation) -> Grammar {
    let bodies = bodies(variation, ReadVariation::default());
    let body = |address: u64| bodies.get(&address).map(Vec::as_slice);
    let input = input(
        "Modifier::Read(CReader&)",
        "Modifier::ReadMember(CReader&, int)",
    );
    analyze(&input, &body).points[&POINT].clone().unwrap()
}

fn stored(callee: &str, kind: ReaderKind, destination: i64) -> ReaderJoin {
    ReaderJoin::Stored {
        callee: callee.into(),
        kind,
        destination,
        repeat: RepeatBehavior::Replace,
    }
}

fn grammar(variation: Variation) -> Grammar {
    analyze_member(variation, "Weight::ReadMember(CReader&, int)")
}

fn key<'a>(grammar: &'a Grammar, name: &str) -> &'a RootField {
    grammar
        .fields
        .iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("no key {name}"))
}

fn stop<'a>(grammar: &'a Grammar, name: &str) -> &'a str {
    grammar
        .stops
        .iter()
        .find(|(key, _)| key.as_deref() == Some(name))
        .map(|(_, stop)| stop.reason)
        .unwrap_or_else(|| panic!("no stop for {name}: {:?}", grammar.stops))
}

fn stored_scope() -> Option<Value> {
    Some(Value::Load(Box::new(Value::Owner(0x30)), 8))
}

#[test]
fn fixed_keys_read_into_the_owner() {
    let grammar = grammar(Variation::default());

    assert!(grammar.stops.is_empty(), "{:?}", grammar.stops);
    assert!(matches!(
        key(&grammar, "base").readers.as_slice(),
        [ReaderJoin::Joined { callee, tail: true, .. }] if callee == "CReader::Read(CFixedPoint&)"
    ));
    assert_eq!(
        key(&grammar, "days").readers,
        [ReaderJoin::Stored {
            callee: "CToken::GetInt() const".into(),
            kind: ReaderKind::Integer,
            destination: 0x10,
            repeat: RepeatBehavior::Replace,
        }]
    );
    assert!(matches!(
        grammar.read_entry.as_ref().map(|entry| &entry.scalar),
        Ok(Some(ReaderJoin::Joined { callee, .. })) if callee == "CReader::Read(CFixedPoint&)"
    ));
}

#[test]
fn operations_come_from_the_switch_and_accumulate() {
    let grammar = grammar(Variation::default());

    assert_eq!(
        grammar.operations,
        [
            Operation {
                key: "add".into(),
                operand: Some(Operand {
                    callee: "CVariableValue::Read(CReader&, EScopeType)".into(),
                    scope: stored_scope(),
                    point: None,
                    destination: None,
                }),
            },
            Operation {
                key: "round".into(),
                operand: None,
            },
        ]
    );
    assert_eq!(grammar.operation_repeat, RepeatBehavior::Accumulate);
}

#[test]
fn an_unknown_key_that_only_flags_an_error_and_logs_is_rejected() {
    let grammar = grammar(Variation::default());

    assert_eq!(grammar.other_keys, OtherKeys::Rejected);
    assert!(!grammar.fields.iter().any(|field| field.name == "potential"));
}

#[test]
fn a_nested_entry_has_its_own_grammar_and_scope() {
    let grammar = grammar(Variation::default());

    let nested = &grammar.nested["modifier"];
    assert_eq!(
        nested.reader.member,
        "Entry::ReadMember(CReader&, int, EScopeType)"
    );
    assert_eq!(grammar.key_scopes.get("modifier"), stored_scope().as_ref());
    let entry = nested.grammar.as_ref().unwrap();
    assert_eq!(entry.operation_repeat, RepeatBehavior::Replace);
    assert_eq!(
        entry.operations[0].operand.as_ref().unwrap().scope,
        Some(Value::EnclosingScope)
    );
    assert_eq!(
        entry.other_keys,
        OtherKeys::Triggers(Some(Value::EnclosingScope))
    );
    assert!(entry.fields.is_empty(), "{:?}", entry.fields);
}

#[test]
fn a_delegating_variant_keeps_inherited_keys_and_its_own_override() {
    let grammar = analyze_member(Variation::default(), "Variant::ReadMember(CReader&, int)");

    assert!(matches!(
        key(&grammar, "base").readers.as_slice(),
        [ReaderJoin::Joined { arguments, .. }] if arguments.get("x1") == Some(&Value::Owner(0x38))
    ));
    assert!(grammar.fields.iter().any(|field| field.name == "days"));
    assert_eq!(grammar.operations.len(), 2);
}

#[test]
fn a_reader_call_without_the_reader_is_a_gap() {
    let grammar = grammar(Variation {
        wrong_reader: true,
        ..Variation::default()
    });

    assert_eq!(stop(&grammar, "base"), "weight-call");
}

#[test]
fn an_entry_that_is_not_inserted_is_a_gap() {
    let grammar = grammar(Variation {
        no_insert: true,
        ..Variation::default()
    });

    assert_eq!(stop(&grammar, "modifier"), "weight-acceptance");
    assert!(!grammar.nested.contains_key("modifier"));
    assert!(grammar.undetermined_keys);
    assert_eq!(grammar.operation_repeat, RepeatBehavior::Unknown);
}

#[test]
fn an_effect_family_fallback_is_not_a_trigger_condition() {
    let bodies = BTreeMap::from([
        (MEMBER, member(Variation::default())),
        (ENTRY_MEMBER, entry_member()),
        (ENUM_SWITCH, switch()),
        (READ, read(ReadVariation::default())),
    ]);
    let body = |address: u64| bodies.get(&address).map(Vec::as_slice);
    let mut input = input(WEIGHT_READ, "Weight::ReadMember(CReader&, int)");
    for family in input.families.values_mut() {
        *family = crate::BlockFamily::Effect;
    }

    let facts = analyze(&input, &body);

    let grammar = facts.points[&POINT].as_ref().unwrap();
    let entry = grammar.nested["modifier"].grammar.as_ref().unwrap();
    assert!(matches!(entry.other_keys, OtherKeys::Unresolved(_)));
}

#[test]
fn an_unmodelled_call_that_receives_the_owner_is_a_gap() {
    let grammar = grammar(Variation {
        owner_call: true,
        ..Variation::default()
    });

    assert_eq!(stop(&grammar, "days"), "weight-call");
}

#[test]
fn an_interior_unnamed_key_leaves_other_keys_unresolved() {
    let grammar = grammar(Variation {
        interior_key: true,
        ..Variation::default()
    });

    assert!(matches!(grammar.other_keys, OtherKeys::Unresolved(_)));
}

#[test]
fn a_stored_scope_is_requested_only_when_the_read_entry_keeps_it() {
    let member = "Weight::ReadMember(CReader&, int)";
    let kept = facts(Variation::default(), member, ReadVariation::default());
    assert_eq!(
        kept.stored_scopes(),
        BTreeMap::from([(POINT, BTreeSet::from([0x30]))])
    );

    let clobbered = facts(
        Variation::default(),
        member,
        ReadVariation {
            clobber: true,
            ..ReadVariation::default()
        },
    );
    assert!(clobbered.stored_scopes().is_empty());
}

#[test]
fn a_location_string_moved_over_the_stored_scope_removes_it() {
    let member = "Weight::ReadMember(CReader&, int)";
    let located = |offset| {
        let read = ReadVariation {
            location: Some(offset),
            ..ReadVariation::default()
        };
        facts(Variation::default(), member, read)
    };

    let apart = located(0x1b8);
    assert_eq!(
        apart.stored_scopes(),
        BTreeMap::from([(POINT, BTreeSet::from([0x30]))])
    );

    let over = located(0x28);
    let grammar = over.points[&POINT].as_ref().unwrap();
    assert!(grammar.read_entry.is_ok(), "{:?}", grammar.read_entry);
    assert!(over.stored_scopes().is_empty());
}

#[test]
fn an_event_target_made_from_the_value_is_stored_where_it_is_assigned() {
    let grammar = modifier_grammar(Variation::default());

    assert_eq!(
        key(&grammar, "scope").readers,
        [stored(
            "CEventTarget::CEventTarget(CToken)",
            ReaderKind::Target,
            0x28
        )]
    );
}

#[test]
fn a_value_compared_with_fixed_names_is_a_keyword() {
    let grammar = modifier_grammar(Variation::default());

    assert_eq!(
        key(&grammar, "calc").readers,
        [stored(
            "Modifier::ReadMember(CReader&, int)",
            ReaderKind::Keyword,
            0x298
        )]
    );
}

#[test]
fn a_keyword_whose_values_store_in_different_words_is_a_gap() {
    let grammar = modifier_grammar(Variation {
        split_keyword: true,
        ..Variation::default()
    });

    assert_eq!(stop(&grammar, "calc"), "weight-acceptance");
}

#[test]
fn a_value_mapped_through_the_switch_is_a_keyword_and_never_an_operation() {
    let grammar = modifier_grammar(Variation::default());

    // The value pass also runs the value 44, which equals the key token of `mode`, and the
    // unknown-name fallback 16 is stored like any other value.
    assert_eq!(
        key(&grammar, "mode").readers,
        [stored(
            "EOperation TokenToEnum<EOperation>(int const&)",
            ReaderKind::Keyword,
            0x2e0
        )]
    );
    assert!(grammar.operations.is_empty(), "{:?}", grammar.operations);
}

#[test]
fn a_trigger_lookup_of_the_value_is_a_reference() {
    let grammar = modifier_grammar(Variation::default());

    assert_eq!(
        key(&grammar, "trigger").readers,
        [stored(readers::TRIGGER_LOOKUP, ReaderKind::Reference, 0x88)]
    );
}

#[test]
fn a_copy_of_some_other_text_into_the_owner_is_a_gap() {
    let grammar = modifier_grammar(Variation {
        other_text: true,
        ..Variation::default()
    });

    assert_eq!(stop(&grammar, "trigger"), "weight-call");
}

#[test]
fn a_key_read_by_the_object_that_another_key_stores_names_that_key() {
    let grammar = modifier_grammar(Variation::default());

    assert_eq!(stop(&grammar, "parameters"), "weight-dependent-reader");
    assert_eq!(
        grammar.dependent_readers,
        BTreeMap::from([("parameters".into(), "trigger".into())])
    );
    assert!(!grammar.undetermined_keys);
}

#[test]
fn a_read_entry_that_records_its_location_and_reads_its_base_has_no_bare_value() {
    let grammar = modifier_grammar(Variation::default());

    let entry = grammar.read_entry.as_ref().unwrap();
    assert_eq!(entry.scalar, None);
    assert!(entry.writes.contains(&0x1b8), "{:?}", entry.writes);
}

#[test]
fn only_the_trigger_reader_gives_a_nested_entry_no_bare_value_unread() {
    let nested_scalar = |entry_read: &str| {
        let bodies = bodies(Variation::default(), ReadVariation::default());
        let body = |address: u64| bodies.get(&address).map(Vec::as_slice);
        let mut input = input(WEIGHT_READ, "Weight::ReadMember(CReader&, int)");
        for symbol in &mut input.symbols {
            if symbol.address == ENTRY_READ {
                symbol.name = entry_read.into();
            }
        }
        let grammar = analyze(&input, &body).points[&POINT].clone().unwrap();
        let entry = grammar.nested["modifier"].grammar.clone().unwrap();
        entry.read_entry.map(|entry| entry.scalar)
    };

    assert_eq!(
        nested_scalar("CTrigger::Read(CReader&, EScopeType)"),
        Ok(None)
    );
    assert!(nested_scalar("CReader::Read(CPersistent&)").is_err());
}

#[test]
fn thunk_names_resolve_to_their_target() {
    assert_eq!(
        thunk_target("{virtual override thunk({offset(-8)}, A::ReadMember(CReader&, int))}"),
        "A::ReadMember(CReader&, int)"
    );
    assert_eq!(
        thunk_target("non-virtual thunk to A::ReadMember(CReader&, int)"),
        "A::ReadMember(CReader&, int)"
    );
    assert_eq!(thunk_target("A::Read(CReader&)"), "A::Read(CReader&)");
}
