use super::*;
use crate::engine::analysis::assembler::{Arm64, arm64};
use crate::engine::analysis::fields::{PathOutcome, RootField, TokenPath};

const START: u64 = 0x1000;
const ASSIGN: &str = "CVariableValue::Assign(CToken const&, EScopeType, CString const&)";
const READ_INT: &str = "CReader::Read(int&)";

/// One authored member body and the keys whose paths run through it.
struct Member {
    code: Vec<u8>,
    fields: Vec<RootField>,
    paths: Vec<TokenPath>,
}

impl Member {
    fn new(code: Arm64) -> Self {
        Self {
            code: code.bytes(),
            fields: Vec::new(),
            paths: Vec::new(),
        }
    }

    /// A key whose path runs from `START` through `entry` to the reader call at `call`.
    fn key(mut self, name: &str, entry: u64, call: u64, callee: &str, tail: bool) -> Self {
        let arguments = if callee == ASSIGN {
            [("x0", Value::Owner(0xa8)), ("x1", Value::Reader(0x278))]
        } else {
            [("x0", Value::Reader(0)), ("x1", Value::Owner(0xa8))]
        }
        .into_iter()
        .chain([("x19", Value::Owner(0))])
        .map(|(register, value)| (register.to_string(), value))
        .collect();
        let join = ReaderJoin::Joined {
            callee: callee.into(),
            arguments,
            tail,
        };
        let instructions = std::iter::once(START)
            .chain((entry..=call).step_by(4))
            .collect();

        self.paths.push(TokenPath {
            domain: [self.paths.len() as i64; 2],
            conditions: Vec::new(),
            instructions,
            terminal: call,
            outcome: PathOutcome::Reader(join.clone()),
        });
        self.fields.push(RootField {
            name: name.into(),
            token: self.fields.len() as i64,
            constructor: 0,
            paths: vec![self.paths.len() - 1],
            readers: vec![join],
        });
        self
    }

    fn groups(&self, execute: Option<Result<Execution, Unresolved>>) -> Vec<Group> {
        let code = |address: u64| {
            let end = START + self.code.len() as u64;
            (START..end)
                .contains(&address)
                .then_some((START, self.code.as_slice()))
        };
        let initial = [(0x2b0, 1), (0x2b1, 0), (0x2b2, 0), (0x2b3, 0)]
            .into_iter()
            .chain((0xa8..0xac).map(|at| (at, 0)))
            .collect();

        groups(
            &self.fields,
            &self.paths,
            &code,
            &initial,
            execute,
            &Ok(Countdown { counts: 0x40 }),
            &BTreeMap::new(),
        )
        .groups
    }
}

fn execute_bindings(operand: &str, factor: &str) -> Option<Result<Execution, Unresolved>> {
    Some(Ok(Execution {
        bindings: [
            ("operand".to_string(), operand.to_string()),
            ("factor".to_string(), factor.to_string()),
        ]
        .into(),
        flag_countdown: true,
    }))
}

fn factors(group: &Group) -> Vec<(&str, Result<Option<i64>, Unresolved>)> {
    group
        .units
        .iter()
        .map(|unit| (unit.key.as_str(), unit.factor.clone()))
        .collect()
}

/// `days` tail-calls the operand reader; `months` and `years` store 30 and 360 to `+0x2b0`.
fn shared_factor_member() -> Member {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x0, x19, #0xa8;
        b extern 0x9000;
        add x0, x19, #0xa8;
        bl extern 0x9000;
        mov w8, #30;
        str w8, [x19, #0x2b0];
        ldp x29, x30, [sp], #16;
        ret;
        add x0, x19, #0xa8;
        bl extern 0x9000;
        mov w8, #360;
        b extern 0x1018
    );

    Member::new(code)
        .key("days", 0x1004, 0x1008, ASSIGN, true)
        .key("months", 0x100c, 0x1010, ASSIGN, false)
        .key("years", 0x1024, 0x1028, ASSIGN, false)
}

#[test]
fn a_shared_factor_needs_the_execute_body_that_multiplies_its_operand_and_slot() {
    let member = shared_factor_member();
    let [group] = member
        .groups(execute_bindings("0xa8", "0x2b0"))
        .try_into()
        .unwrap();

    assert_eq!(group.destination, 0xa8);
    assert_eq!(
        factors(&group),
        [
            ("days", Ok(None)),
            ("months", Ok(Some(30))),
            ("years", Ok(Some(360)))
        ]
    );
    assert_eq!(
        group.combination,
        Ok(Combination::SharedFactor {
            factor_slot: 0x2b0,
            initial_factor: 1
        })
    );
    assert_eq!(group.consumption, Ok(Consumption::FlagCountdown));

    for execute in [
        None,
        Some(Err(Unresolved::new("duration-execute-body"))),
        execute_bindings("0xb0", "0x2b0"),
        execute_bindings("0xa8", "0x2b8"),
    ] {
        let [group] = member.groups(execute).try_into().unwrap();

        assert!(group.combination.is_err());
        assert!(group.consumption.is_err());
        assert_eq!(factors(&group)[1], ("months", Ok(Some(30))));
    }
}

#[test]
fn a_scaled_read_multiplies_its_destination_in_place() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x1, x19, #0xa8;
        b extern 0x9000;
        add x1, x19, #0xa8;
        bl extern 0x9000;
        ldr w8, [x19, #0xa8];
        lsl w9, w8, #5;
        sub w8, w9, w8, lsl #1;
        str w8, [x19, #0xa8];
        ldp x20, x19, [sp, #0x10];
        ldp x29, x30, [sp], #0x20;
        ret;
        add x1, x19, #0xa8;
        bl extern 0x9000;
        ldr w8, [x19, #0xa8];
        mov w9, #0x168;
        mul w8, w8, w9;
        str w8, [x19, #0xa8];
        add sp, sp, #0x20;
        ret
    );
    let member = Member::new(code)
        .key("days", 0x1004, 0x1008, READ_INT, true)
        .key("months", 0x100c, 0x1010, READ_INT, false)
        .key("years", 0x1030, 0x1034, READ_INT, false);
    let [group] = member.groups(None).try_into().unwrap();

    assert_eq!(
        factors(&group),
        [
            ("days", Ok(Some(1))),
            ("months", Ok(Some(30))),
            ("years", Ok(Some(360)))
        ]
    );
    assert_eq!(group.combination, Ok(Combination::ScaledAtRead));
    assert!(group.consumption.is_err());
    assert_eq!(word(&group.initial, group.destination), Some(0));
}

#[test]
fn a_store_before_a_tail_call_leaves_the_key_unresolved() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        mov w8, #1;
        str w8, [x19, #0x2b0];
        add x0, x19, #0xa8;
        b extern 0x9000;
        add x0, x19, #0xa8;
        bl extern 0x9000;
        mov w8, #30;
        str w8, [x19, #0x2b0];
        ret
    );
    let member = Member::new(code)
        .key("days", 0x1004, 0x1010, ASSIGN, true)
        .key("months", 0x1014, 0x1018, ASSIGN, false);
    let [group] = member
        .groups(execute_bindings("0xa8", "0x2b0"))
        .try_into()
        .unwrap();

    assert_eq!(
        factors(&group)[0],
        ("days", Err(Unresolved::new("duration-prefix-store")))
    );
    assert!(group.combination.is_err());
}

#[test]
fn factor_stores_to_different_slots_do_not_combine() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x0, x19, #0xa8;
        bl extern 0x9000;
        mov w8, #30;
        str w8, [x19, #0x2b0];
        ret;
        add x0, x19, #0xa8;
        bl extern 0x9000;
        mov w8, #360;
        str w8, [x19, #0x2b4];
        ret
    );
    let member = Member::new(code)
        .key("months", 0x1004, 0x1008, ASSIGN, false)
        .key("years", 0x1018, 0x101c, ASSIGN, false);
    let [group] = member
        .groups(execute_bindings("0xa8", "0x2b0"))
        .try_into()
        .unwrap();

    assert_eq!(
        group.combination,
        Err(Unresolved::new("duration-mixed-keys"))
    );
}

#[test]
fn a_later_overlapping_store_replaces_the_factor() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x0, x19, #0xa8;
        bl extern 0x9000;
        mov w8, #30;
        str w8, [x19, #0x2b0];
        mov w8, #7;
        str w8, [x19, #0x2b0];
        ret
    );
    let member = Member::new(code).key("months", 0x1004, 0x1008, ASSIGN, false);
    let [group] = member
        .groups(execute_bindings("0xa8", "0x2b0"))
        .try_into()
        .unwrap();

    assert_eq!(factors(&group), [("months", Ok(Some(7)))]);
}

#[test]
fn an_unknown_continuation_instruction_leaves_the_key_unresolved() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x1, x19, #0xa8;
        bl extern 0x9000;
        ldr w8, [x19, #0xa8];
        udiv w8, w8, w9;
        str w8, [x19, #0xa8];
        ret;
        add x1, x19, #0xa8;
        bl extern 0x9000;
        mov w8, #30;
        str w8, [x19, #0x2b0];
        ret
    );
    let member = Member::new(code)
        .key("months", 0x1004, 0x1008, READ_INT, false)
        .key("years", 0x101c, 0x1020, READ_INT, false);
    let [group] = member.groups(None).try_into().unwrap();

    assert_eq!(
        factors(&group)[0],
        ("months", Err(Unresolved::new("duration-post-read")))
    );
    assert!(group.combination.is_err());
}

/// Two stack slots transfer scaled counts to the same owner word.
fn stack_member() -> Member {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x1, sp, #0xc;
        bl extern 0x9000;
        ldr w8, [sp, #0xc];
        lsl w9, w8, #5;
        sub w8, w9, w8, lsl #1;
        str w8, [x19, #0x470];
        ret;
        add x1, sp, #8;
        bl extern 0x9000;
        ldr w8, [sp, #8];
        mov w9, #360;
        mul w8, w8, w9;
        str w8, [x19, #0x470];
        ret
    );
    let mut member = Member::new(code)
        .key("short_unit", 0x1004, 0x1008, READ_INT, false)
        .key("long_unit", 0x1020, 0x1024, READ_INT, false);

    for (field, offset) in member.fields.iter_mut().zip([0xc, 8]) {
        let ReaderJoin::Joined { arguments, .. } = &mut field.readers[0] else {
            unreachable!()
        };
        arguments.insert("x1".into(), Value::Stack(offset));
        arguments.insert("sp".into(), Value::Stack(0));
    }

    member
}

#[test]
fn stack_temporaries_join_at_the_final_owner_store() {
    let [group] = stack_member().groups(None).try_into().unwrap();
    assert_eq!(group.destination, 0x470);
    assert_eq!(group.combination, Ok(Combination::ScaledAtRead));
    assert_eq!(
        factors(&group),
        [("short_unit", Ok(Some(30))), ("long_unit", Ok(Some(360))),]
    );
}

#[test]
fn a_stack_transfer_requires_the_read_slot_and_owner_provenance() {
    for missing_owner in [false, true] {
        let mut member = stack_member();
        let ReaderJoin::Joined { arguments, .. } = &mut member.fields[0].readers[0] else {
            unreachable!()
        };
        let reason = if missing_owner {
            arguments.remove("x19");
            "duration-post-read-store"
        } else {
            arguments.insert("x1".into(), Value::Stack(0x10));
            "duration-stack-store"
        };
        let code = |_: u64| Some((START, member.code.as_slice()));
        let inventory = groups(
            &member.fields[..1],
            &member.paths,
            &code,
            &BTreeMap::new(),
            None,
            &Ok(Countdown { counts: 0x40 }),
            &BTreeMap::new(),
        );
        assert!(inventory.groups.is_empty());
        assert_eq!(inventory.unresolved[0].reason, reason);
    }
}

#[test]
fn a_stack_write_cannot_be_ignored_between_the_reader_and_the_owner_store() {
    let mut member = stack_member();
    let overwrite = arm64!(at 0x100c; str wzr, [sp, #0xc]);
    member.code[12..16].copy_from_slice(&overwrite);
    let inventory = groups(
        &member.fields[..1],
        &member.paths,
        &|_: u64| Some((START, member.code.as_slice())),
        &BTreeMap::new(),
        None,
        &Ok(Countdown { counts: 0x40 }),
        &BTreeMap::new(),
    );
    assert!(inventory.groups.is_empty());
    assert_eq!(inventory.unresolved[0].reason, "duration-stack-overwrite");
}

#[test]
fn a_byte_presence_store_is_not_a_word_factor() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x0, x19, #0xa8;
        bl extern 0x9000;
        mov w8, #1;
        strb w8, [x19, #0x2b0];
        ret
    );
    let member = Member::new(code).key("value", 0x1004, 0x1008, ASSIGN, false);
    assert!(member.groups(None).is_empty());
}

#[test]
fn a_byte_write_overlapping_the_count_is_unresolved() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x1, x19, #0xa8;
        bl extern 0x9000;
        mov w8, #1;
        strb w8, [x19, #0xa9];
        ret
    );
    let member = Member::new(code).key("value", 0x1004, 0x1008, READ_INT, false);
    let inventory = groups(
        &member.fields,
        &member.paths,
        &|_: u64| Some((START, member.code.as_slice())),
        &BTreeMap::new(),
        None,
        &Ok(Countdown { counts: 0x40 }),
        &BTreeMap::new(),
    );
    assert_eq!(inventory.unresolved[0].reason, "duration-post-read-store");
}

#[test]
fn a_prefix_reset_overwritten_by_the_integer_reader_forms_no_group() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        mov w8, #1;
        strb w8, [x19, #0xa4];
        str wzr, [x19, #0xa8];
        add x1, x19, #0xa8;
        b extern 0x9000
    );
    let member = Member::new(code).key("value", 0x1004, 0x1014, READ_INT, true);
    let inventory = groups(
        &member.fields,
        &member.paths,
        &|_: u64| Some((START, member.code.as_slice())),
        &BTreeMap::new(),
        None,
        &Ok(Countdown { counts: 0x40 }),
        &BTreeMap::new(),
    );
    assert!(inventory.groups.is_empty());
    assert!(inventory.unresolved.is_empty());
}

#[test]
fn keys_without_a_factor_form_no_group() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x1, x19, #0xa8;
        b extern 0x9000
    );
    let member = Member::new(code).key("days", 0x1004, 0x1008, READ_INT, true);

    assert!(member.groups(None).is_empty());
}

#[test]
fn a_missing_initial_factor_leaves_the_shared_combination_unresolved() {
    let member = shared_factor_member();
    let code = |address: u64| {
        (START..START + member.code.len() as u64)
            .contains(&address)
            .then_some((START, member.code.as_slice()))
    };
    let [group] = groups(
        &member.fields,
        &member.paths,
        &code,
        &BTreeMap::new(),
        execute_bindings("0xa8", "0x2b0"),
        &Ok(Countdown { counts: 0x40 }),
        &BTreeMap::new(),
    )
    .groups
    .try_into()
    .unwrap();

    assert_eq!(
        group.combination,
        Err(Unresolved::new("duration-initial-state"))
    );
}

#[test]
fn a_missing_countdown_proof_keeps_the_combination_but_not_the_consumption() {
    let member = shared_factor_member();
    let code = |address: u64| {
        (START..START + member.code.len() as u64)
            .contains(&address)
            .then_some((START, member.code.as_slice()))
    };
    let initial = (0x2b0..0x2b4)
        .map(|at| (at, u8::from(at == 0x2b0)))
        .collect();
    let [group] = groups(
        &member.fields,
        &member.paths,
        &code,
        &initial,
        execute_bindings("0xa8", "0x2b0"),
        &Err(Unresolved::new("duration-flag-update")),
        &BTreeMap::new(),
    )
    .groups
    .try_into()
    .unwrap();

    assert!(group.combination.is_ok());
    assert_eq!(
        group.consumption,
        Err(Unresolved::new("duration-flag-update"))
    );
}

#[test]
fn a_candidate_whose_code_cannot_be_followed_is_kept_as_unresolved() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x1, x19, #0xa8;
        bl extern 0x9000;
        bl extern 0x9100;
        ret
    );
    let member = Member::new(code).key("months", 0x1004, 0x1008, READ_INT, false);
    let inventory = groups(
        &member.fields,
        &member.paths,
        &|address: u64| {
            (START..START + member.code.len() as u64)
                .contains(&address)
                .then_some((START, member.code.as_slice()))
        },
        &BTreeMap::new(),
        None,
        &Ok(Countdown { counts: 0x40 }),
        &BTreeMap::new(),
    );

    assert!(inventory.groups.is_empty());
    assert_eq!(
        inventory.unresolved,
        [Unresolved::new("duration-post-read")]
    );
}

#[test]
fn register_only_instructions_do_not_hide_a_factor() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x0, x19, #0xa8;
        bl extern 0x9000;
        udiv w10, w11, w12;
        ldr x9, [x20], #8;
        mov w8, #30;
        str w8, [x19, #0x2b0];
        str x9, [sp, #0x10];
        ret
    );
    let member = Member::new(code).key("months", 0x1004, 0x1008, ASSIGN, false);
    let [group] = member
        .groups(execute_bindings("0xa8", "0x2b0"))
        .try_into()
        .unwrap();

    assert_eq!(factors(&group), [("months", Ok(Some(30)))]);
}

/// The exact-build bodies, each mutated so that its proof must fail.
#[test]
#[ignore = "requires the exact supported executable through STELLARIS_PATH"]
fn m45_duration_consumption_proofs() {
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let analysis = native.bound().analysis.as_ref().unwrap();
    let (input, _) = analysis
        .grammar_input(crate::DeclarationKind::Effect)
        .unwrap();
    let (set_flag, update_flags, execute_rows) = analysis.duration_bodies_for_test().unwrap();
    let names = &input.durations.names;

    assert_eq!(
        countdown(&set_flag, &update_flags, names),
        Ok(Countdown { counts: 0x40 })
    );
    let bindings = execute(&execute_rows, names).unwrap();
    assert_eq!(bindings.bindings["operand"], "0xa8");
    assert_eq!(bindings.bindings["factor"], "0x2b0");

    let mutated = |rows: &[Instruction], from: &str, to: &str| {
        let mut rows = rows.to_vec();
        let row = rows
            .iter_mut()
            .find(|row| format!("{} {}", row.operation, row.operands).starts_with(from))
            .unwrap();
        row.operation = to
            .split_once(' ')
            .map_or(to, |(operation, _)| operation)
            .into();
        if let Some((_, operands)) = to.split_once(' ') {
            row.operands = operands.into();
        }
        rows
    };

    let other_slot = execute(
        &mutated(&execute_rows, "ldr w8,[x19,#0x2b0]", "ldr w8,[x19,#0x2b8]"),
        names,
    )
    .unwrap();
    assert_eq!(other_slot.bindings["factor"], "0x2b8");

    for (from, to) in [("mul w3", "add w3,w8,w0"), ("mov w4,#0", "mov w4,#1")] {
        assert!(
            execute(&mutated(&execute_rows, from, to), names).is_err(),
            "{from}"
        );
    }

    let without_multiply: Vec<_> = execute_rows
        .iter()
        .filter(|row| row.operation != "mul")
        .cloned()
        .collect();
    assert!(execute(&without_multiply, names).is_err());

    for (from, to) in [("tbnz", "tbz"), ("subs", "sub")] {
        assert!(
            countdown(&set_flag, &mutated(&update_flags, from, to), names).is_err(),
            "{from}"
        );
    }
    assert!(
        countdown(
            &mutated(&set_flag, "cmp w4,#1", "cmp w4,#0"),
            &update_flags,
            names
        )
        .is_err()
    );
}

/// Every command in the tracked expected file, with its public durations and their gaps.
#[test]
#[ignore = "requires the exact supported executable through STELLARIS_PATH"]
fn m45_duration_static_parity() {
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let expected: serde_json::Map<String, serde_json::Value> = serde_json::from_str(include_str!(
        "../../../../tests/expected/duration-m45/static.json"
    ))
    .unwrap();
    let mut actual = serde_json::Map::new();

    for command in expected.keys() {
        let (kind, name) = command.split_once('/').unwrap();
        let kind = match kind {
            "effect" => crate::DeclarationKind::Effect,
            "trigger" => crate::DeclarationKind::Trigger,
            _ => panic!("{command}"),
        };
        let answer = native.command_grammar(kind, name).unwrap();
        let gaps: Vec<_> = answer
            .gaps
            .iter()
            .filter(|gap| gap.detail.starts_with("Duration keys"))
            .map(|gap| format!("{:?}: {}", gap.kind, gap.detail))
            .collect();

        actual.insert(
            command.clone(),
            serde_json::json!({"durations": answer.value.durations, "gaps": gaps}),
        );
    }

    if let Some(path) = std::env::var_os("NATIVE_DURATION_REPORT") {
        std::fs::write(path, serde_json::to_string_pretty(&actual).unwrap() + "\n").unwrap();
    }

    assert_eq!(actual, expected);
}

#[test]
fn the_authored_relation_execute_requires_the_product_and_consumer() {
    let bytes = arm64!(at START;
        sub sp,sp,#0x1b0;
        stp x19,x20,[sp,#0x170];
        stp x21,x22,[sp,#0x180];
        stp x23,x24,[sp,#0x190];
        stp x29,x30,[sp,#0x1a0];
        add x29,sp,#0x1a0;
        mov x23,x1;
        mov x24,x0;
        add x0,x0,#0xa8;
        mov x25,sp;
        mov x2,#0x0;
        bl extern 0x9000;
        mov x0,sp;
        bl extern 0x9100;
        mov x22,x0;
        mov x0,sp;
        bl extern 0x9200;
        ldr x25,[x22];
        ldr x25,[x25,#0x58];
        mov x0,x22;
        blr x25;
        tbz w0,#0,extern 0x10f0;
        ldrsb w25,[x24,#0x257];
        tbnz w25,#0x1f,extern 0x1084;
        and x25,x25,#0xff;
        cbz x25,extern 0x108c;
        add x2,x24,#0x240;
        add x1,x24,#0x268;
        add x3,x24,#0x28;
        mov x0,x23;
        bl extern 0x9300;
        mov x21,x0;
        b extern 0x1090;
        ldr x25,[x24,#0x248];
        cbnz x25,extern 0x1068;
        ldrh w21,[x24,#0x238];
        mov x0,x23;
        bl extern 0x9400;
        mov x1,x22;
        bl extern 0x9500;
        mov x22,x0;
        ldr x25,[x0];
        ldr x25,[x25,#0x40];
        blr x25;
        cbz w0,extern 0x10f0;
        add x22,x22,#0x38;
        adrp x25,extern 0x8000;
        add x25,x25,#0;
        ldr x25,[x25];
        add x20,x25,#0xb8;
        add x0,x24,#0x3f8;
        mov x1,x23;
        bl extern 0x9700;
        ldr w25,[x24,#0x600];
        mul w3,w25,w0;
        and x1,x21,#0xffff;
        mov x0,x22;
        mov x2,x20;
        mov w4,#0x0;
        bl extern 0x9800;
        ldp x29,x30,[sp,#0x1a0];
        ldp x23,x24,[sp,#0x190];
        ldp x21,x22,[sp,#0x180];
        ldp x19,x20,[sp,#0x170];
        add sp,sp,#0x1b0;
        ret;
        mov x24,x0;
        mov x0,sp;
        bl extern 0x9200;
        mov x0,x24;
        bl extern 0x9900
    );
    let names = BTreeMap::from([
        (0x9000, "CEventTarget::GetScope(CEventScope&, char const*) const".into()),
        (0x9100, "CScopeObjectReference::GetCountry() const".into()),
        (0x9200, "CEventScope::~CEventScope()".into()),
        (0x9300, "CreateDynamicFlag(CEventScope&, CEventTarget const&, CString const&, CString const&)".into()),
        (0x9400, "CScopeObjectReference::AccessCountry()".into()),
        (0x9500, "CCountry::AccessOrCreateNewRelation(CCountry const*)".into()),
        (0x8000, "_g_CurrentGameState".into()),
        (0x9700, "CIntVariableValue::GetValue(CEventScope&) const".into()),
        (0x9800, "CPdxIntegerFlags::SetFlag(CPdxIntegerFlags::CIntFlag<unsigned short>, CDate const&, int, CPdxIntegerFlags::ESetFlagMode)".into()),
        (0x9900, "__Unwind_Resume".into())
    ]);
    let rows = decode_arm64(&bytes, START).unwrap();
    let proof = execute(&rows, &names).unwrap();
    assert_eq!(proof.bindings["operand"], "0x3f8");
    assert_eq!(proof.bindings["factor"], "0x600");
    assert!(proof.flag_countdown);
    for operation in ["mul", "consumer"] {
        let mut missing = rows.clone();
        let row = missing
            .iter_mut()
            .find(|row| {
                row.operation == operation
                    || (operation == "consumer"
                        && row.operation == "bl"
                        && number(&row.operands) == Some(0x9800))
            })
            .unwrap();
        row.operation = "nop".into();
        row.operands.clear();
        assert!(execute(&missing, &names).is_err());
    }
}

#[test]
fn the_authored_trait_execute_requires_the_product_and_consumer() {
    let bytes = arm64!(at START;
        stp x19,x20,[sp,#-0x30]!;
        stp x21,x22,[sp,#0x10];
        stp x29,x30,[sp,#0x20];
        add x29,sp,#0x20;
        mov x20,x1;
        mov x22,x0;
        mov x0,x1;
        bl extern 0x9000;
        mov x21,x0;
        ldr x23,[x0];
        ldr x23,[x23,#0x40];
        blr x23;
        cbz w0,extern 0x1064;
        ldr x19,[x22,#0x2b0];
        add x0,x22,#0xa8;
        mov x1,x20;
        bl extern 0x9100;
        ldr w23,[x22,#0x2e0];
        mul w2,w23,w0;
        mov x0,x21;
        mov x1,x19;
        ldp x29,x30,[sp,#0x20];
        ldp x21,x22,[sp,#0x10];
        ldp x19,x20,[sp],#0x30;
        b extern 0x9200;
        ldp x29,x30,[sp,#0x20];
        ldp x21,x22,[sp,#0x10];
        ldp x19,x20,[sp],#0x30;
        ret
    );
    let names = BTreeMap::from([
        (0x9000, "CScopeObjectReference::AccessLeader()".into()),
        (
            0x9100,
            "CIntVariableValue::GetValue(CEventScope&) const".into(),
        ),
        (0x9200, "CLeader::AddTimedTrait(CTrait const*, int)".into()),
    ]);
    let rows = decode_arm64(&bytes, START).unwrap();
    let proof = execute(&rows, &names).unwrap();
    assert_eq!(proof.bindings["operand"], "0xa8");
    assert_eq!(proof.bindings["factor"], "0x2e0");
    assert!(!proof.flag_countdown);
    for operation in ["mul", "b"] {
        let mut missing = rows.clone();
        let row = missing
            .iter_mut()
            .find(|row| row.operation == operation)
            .unwrap();
        row.operation = "nop".into();
        row.operands.clear();
        assert!(execute(&missing, &names).is_err());
    }
}

/// Each installed-build path added for stack transfers, byte presence and overwritten resets.
#[test]
#[ignore = "requires the exact supported executable through STELLARIS_PATH"]
fn m45_duration_stack_and_presence_parity() {
    use crate::{DeclarationKind, DurationCombination, GrammarProperty};
    let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
    let scoped = native
        .scoped_numeric_facts(crate::Operation::CommandGrammar)
        .unwrap();
    let numeric = native
        .numeric_facts(crate::Operation::CommandGrammar)
        .unwrap();
    let spans = scoped_storage(scoped, numeric);
    let mut checked = 0;
    for (&point, subtype) in &scoped.subtypes {
        let Ok(crate::engine::analysis::scoped_numeric::Subtype::Numeric { token_reader, .. }) =
            subtype
        else {
            continue;
        };
        let span = match token_reader.as_str() {
            "CToken::ReadValue(int&) const" => 0x204,
            "CToken::ReadValue(CFixedPoint&) const" => 0x208,
            _ => panic!("unreviewed numeric subtype: {token_reader}"),
        };
        assert_eq!(spans[&point], Ok(span));
        checked += 1;
    }
    assert_eq!(checked, 2);
    let events = [
        "agreement_event",
        "astral_rift_event",
        "bypass_event",
        "carrier_event",
        "colony_event",
        "cosmic_storm_event",
        "cosmic_storm_influence_field_event",
        "country_event",
        "espionage_operation_event",
        "first_contact_event",
        "fleet_event",
        "leader_event",
        "observer_event",
        "planet_event",
        "pop_faction_event",
        "pop_group_event",
        "ship_event",
        "situation_event",
        "starbase_event",
        "system_event",
    ];
    for (kind, name) in events
        .into_iter()
        .map(|name| (DeclarationKind::Effect, name))
        .chain([(DeclarationKind::Trigger, "has_passed_resolution")])
    {
        let answer = native.command_grammar(kind, name).unwrap();
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == crate::GapKind::ReaderSemantics
                    && gap.detail.contains("duration-scoped-literal")),
            "{name}"
        );
        let GrammarProperty::Partial(groups) = answer.value.durations else {
            panic!("{name}: duration list unresolved")
        };
        let [group] = groups.as_slice() else {
            panic!("{name}: {groups:?}")
        };
        assert_eq!(
            group.combination,
            GrammarProperty::Known(DurationCombination::ScaledAtRead),
            "{name}"
        );
        assert_eq!(
            group
                .units
                .iter()
                .map(|unit| (unit.key.as_str(), unit.factor.clone()))
                .collect::<Vec<_>>(),
            [
                ("months", GrammarProperty::Known(Some(30))),
                ("years", GrammarProperty::Known(Some(360))),
            ],
            "{name}"
        );
        assert_eq!(group.omitted_count, GrammarProperty::Known(0), "{name}");
    }
    for name in ["transfer_resources_to_empire", "while"] {
        let answer = native
            .command_grammar(DeclarationKind::Effect, name)
            .unwrap();
        let (GrammarProperty::Known(groups) | GrammarProperty::Partial(groups)) =
            answer.value.durations
        else {
            panic!("{name}: duration list unresolved")
        };
        assert!(groups.is_empty(), "{name}: {groups:?}");
        assert!(
            !answer
                .gaps
                .iter()
                .any(|gap| gap.detail.starts_with("The code after a key's reader")),
            "{name}"
        );
    }
    for registry in ["common/council_agendas", "common/governments/authorities"] {
        let owners = crate::internals::duration_groups::registry(&native, registry).unwrap();
        assert!(!owners.is_empty());
        for owner in owners {
            assert!(owner.inventory.groups.is_empty(), "{registry}: {owner:?}");
            assert!(
                owner.inventory.unresolved.is_empty(),
                "{registry}: {owner:?}"
            );
        }
    }
    for (name, combination, consumption) in [
        (
            "set_timed_relation_flag",
            GrammarProperty::Known(DurationCombination::SharedFactor { initial_factor: 1 }),
            GrammarProperty::Known(crate::DurationConsumption::FlagCountdown),
        ),
        (
            "add_timed_trait",
            GrammarProperty::Known(DurationCombination::SharedFactor { initial_factor: 1 }),
            GrammarProperty::Unresolved,
        ),
    ] {
        let answer = native
            .command_grammar(DeclarationKind::Effect, name)
            .unwrap();
        let (GrammarProperty::Known(groups) | GrammarProperty::Partial(groups)) =
            answer.value.durations
        else {
            panic!("{name}: duration list unresolved")
        };
        assert_eq!(groups[0].combination, combination);
        assert_eq!(groups[0].omitted_count, GrammarProperty::Known(0));
        assert_eq!(groups[0].consumption, consumption);
    }
}

#[test]
fn a_byte_reset_of_a_siblings_factor_cannot_mean_preservation() {
    for prefix in [true, false] {
        let mut member = shared_factor_member();
        let reset = arm64!(at START;
            mov x19, x0;
            mov w8, #1;
            strb w8, [x19, #0x2b0];
            add x0, x19, #0xa8;
            b extern 0x9000
        );
        if prefix {
            member.code.splice(0..0, reset);
            // Keep only the new prefix path and one factor path shifted by the prefix body.
            member.fields.clear();
            member.paths.clear();
            member = member
                .key("plain", 0x1004, 0x1010, ASSIGN, true)
                .key("scaled", 0x1020, 0x1024, ASSIGN, false);
        } else {
            let bytes = arm64!(at 0x1000;
                mov x19, x0;
                add x0, x19, #0xa8;
                bl extern 0x9000;
                mov w8, #1;
                strb w8, [x19, #0x2b0];
                ret;
                add x0, x19, #0xa8;
                bl extern 0x9000;
                mov w8, #30;
                str w8, [x19, #0x2b0];
                ret
            );
            member.code = bytes;
            member.fields.clear();
            member.paths.clear();
            member = member
                .key("plain", 0x1004, 0x1008, ASSIGN, false)
                .key("scaled", 0x1018, 0x101c, ASSIGN, false);
        }
        let [group] = member
            .groups(execute_bindings("0xa8", "0x2b0"))
            .try_into()
            .unwrap();
        assert_eq!(
            group.units[0].factor,
            Err(Unresolved::new("duration-byte-factor"))
        );
        assert!(group.combination.is_err());
    }
}

#[test]
fn conditional_presence_writes_agree_when_the_count_and_factors_agree() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        mov w8, #1;
        strb w8, [x19, #0xa4];
        str wzr, [x19, #0xa8];
        add x1, x19, #0xa8;
        b extern 0x9000;
        str wzr, [x19, #0xa8];
        add x1, x19, #0xa8;
        b extern 0x9000
    );
    let mut member = Member::new(code)
        .key("value", 0x1004, 0x1014, READ_INT, true)
        .key("alternate", 0x1018, 0x1020, READ_INT, true);
    let alternate = member.fields.pop().unwrap();
    member.fields[0].paths.extend(alternate.paths);
    member.fields[0].readers.extend(alternate.readers);
    let inventory = groups(
        &member.fields,
        &member.paths,
        &|_: u64| Some((START, member.code.as_slice())),
        &BTreeMap::new(),
        None,
        &Ok(Countdown { counts: 0x40 }),
        &BTreeMap::new(),
    );
    assert!(inventory.groups.is_empty());
    assert!(inventory.unresolved.is_empty());

    let other_word = arm64!(at 0x1018; str wzr, [x19, #0x2b0]);
    member.code[24..28].copy_from_slice(&other_word);
    let inventory = groups(
        &member.fields,
        &member.paths,
        &|_: u64| Some((START, member.code.as_slice())),
        &BTreeMap::new(),
        None,
        &Ok(Countdown { counts: 0x40 }),
        &BTreeMap::new(),
    );
    assert_eq!(inventory.unresolved[0].reason, "duration-key-alternatives");
}

#[test]
fn a_prefix_restore_cannot_leave_stale_owner_provenance() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        ldp x20, x19, [sp, #0x10];
        str wzr, [x19, #0xa8];
        b extern 0x9000
    );
    let member = Member::new(code).key("value", 0x1004, 0x100c, READ_INT, true);
    let inventory = groups(
        &member.fields,
        &member.paths,
        &|_: u64| Some((START, member.code.as_slice())),
        &BTreeMap::new(),
        None,
        &Ok(Countdown { counts: 0x40 }),
        &BTreeMap::new(),
    );
    assert_eq!(inventory.unresolved[0].reason, "duration-prefix-store");
}

#[test]
fn an_unscaled_stack_transfer_is_not_a_duration_group() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x1, sp, #0xc;
        bl extern 0x9000;
        ldr w8, [sp, #0xc];
        str w8, [x19, #0x470];
        ret
    );
    let mut member = Member::new(code).key("count", 0x1004, 0x1008, READ_INT, false);
    let ReaderJoin::Joined { arguments, .. } = &mut member.fields[0].readers[0] else {
        unreachable!()
    };
    arguments.insert("x1".into(), Value::Stack(0xc));
    arguments.insert("sp".into(), Value::Stack(0));
    let inventory = groups(
        &member.fields,
        &member.paths,
        &|_| Some((START, member.code.as_slice())),
        &BTreeMap::new(),
        None,
        &Ok(Countdown { counts: 0x40 }),
        &BTreeMap::new(),
    );
    assert!(inventory.groups.is_empty());
    assert!(inventory.unresolved.is_empty());

    let mut scaled = stack_member();
    let unscaled = arm64!(at 0x1010; mov w9, w8; mov w8, w8);
    scaled.code[16..24].copy_from_slice(&unscaled);
    let [group] = scaled.groups(None).try_into().unwrap();
    assert_eq!(
        factors(&group),
        [("short_unit", Ok(Some(1))), ("long_unit", Ok(Some(360)))]
    );
}

#[test]
fn indexed_accesses_do_not_preserve_stack_addresses() {
    for access in [
        arm64!(at 0x100c; ldr w10, [x20, #4]!),
        arm64!(at 0x100c; ldr w10, [x20], #4),
        arm64!(at 0x100c; str w10, [x20, #4]!),
        arm64!(at 0x100c; str w10, [x20], #4),
    ] {
        let mut code = Arm64::at(START);
        arm64!(code;
            mov x19, x0;
            add x1, sp, #0xc;
            bl extern 0x9000;
            mov w10, #0;
            ldr w8, [x20, #0xc];
            mov w9, #30;
            mul w8, w8, w9;
            str w8, [x19, #0x470];
            ret
        );
        let mut member = Member::new(code).key("count", 0x1004, 0x1008, READ_INT, false);
        let ReaderJoin::Joined { arguments, .. } = &mut member.fields[0].readers[0] else {
            unreachable!()
        };
        arguments.insert("x1".into(), Value::Stack(0xc));
        arguments.insert("x20".into(), Value::Stack(0));
        let inventory = |member: &Member| {
            groups(
                &member.fields,
                &member.paths,
                &|_| Some((START, member.code.as_slice())),
                &BTreeMap::new(),
                None,
                &Ok(Countdown { counts: 0x40 }),
                &BTreeMap::new(),
            )
        };
        assert_eq!(inventory(&member).groups.len(), 1);
        member.code[12..16].copy_from_slice(&access);
        let result = inventory(&member);
        assert!(result.groups.is_empty());
        assert!(!result.unresolved.is_empty());
    }
}

#[test]
fn a_prefix_word_reset_requires_a_word_integer_reader() {
    for callee in ["CReader::Read(short&)", ASSIGN] {
        let mut code = Arm64::at(START);
        arm64!(code;
            mov x19, x0;
            str wzr, [x19, #0xa8];
            add x1, x19, #0xa8;
            b extern 0x9000
        );
        let member = Member::new(code).key("value", 0x1004, 0x100c, callee, true);
        let inventory = groups(
            &member.fields,
            &member.paths,
            &|_| Some((START, member.code.as_slice())),
            &BTreeMap::new(),
            None,
            &Ok(Countdown { counts: 0x40 }),
            &BTreeMap::new(),
        );
        assert!(inventory.groups.is_empty(), "{callee}");
        assert_eq!(
            inventory.unresolved[0].reason, "duration-prefix-store",
            "{callee}"
        );
    }
}

/// One shared-factor key writes a presence byte; its sibling proves the factor.
fn scoped_byte_member(prefix: bool, byte: i64) -> Member {
    let mut code = Arm64::at(START);
    let byte = byte as u32;
    arm64!(code; mov x19, x0);
    if prefix {
        arm64!(code;
            mov w8, #1;
            strb w8, [x19, #byte];
            add x0, x19, #0xa8;
            b extern 0x9000
        );
    } else {
        arm64!(code;
            add x0, x19, #0xa8;
            bl extern 0x9000;
            mov w8, #1;
            strb w8, [x19, #byte];
            ret
        );
    }
    let sibling = code.here();
    arm64!(code;
        add x0, x19, #0xa8;
        bl extern 0x9000;
        mov w8, #30;
        str w8, [x19, #0x2b0];
        ret
    );
    Member::new(code)
        .key(
            "plain",
            0x1004,
            if prefix { 0x1010 } else { 0x1008 },
            ASSIGN,
            prefix,
        )
        .key("scaled", sibling, sibling + 4, ASSIGN, false)
}

#[test]
fn scoped_literal_byte_writes_need_proved_disjointness() {
    for prefix in [false, true] {
        for literal_byte in 0x2a8..0x2ac {
            let member = scoped_byte_member(prefix, literal_byte);
            let [group] = groups(
                &member.fields,
                &member.paths,
                &|_| Some((START, member.code.as_slice())),
                &BTreeMap::from([(0x2b0, 1), (0x2b1, 0), (0x2b2, 0), (0x2b3, 0)]),
                execute_bindings("0xa8", "0x2b0"),
                &Ok(Countdown { counts: 0x40 }),
                &BTreeMap::from([(0xa8, Ok(0x204))]),
            )
            .groups
            .try_into()
            .unwrap();
            assert_eq!(
                group.units[0].factor,
                Err(Unresolved::new("duration-byte-factor"))
            );
            assert!(group.combination.is_err());
        }
        let member = scoped_byte_member(prefix, 0x2ac);
        for (storage, expected) in [
            (
                BTreeMap::new(),
                Err(Unresolved::new("duration-byte-storage")),
            ),
            (
                BTreeMap::from([(0xa8, Err(Unresolved::new("scoped-missing")))]),
                Err(Unresolved::new("duration-byte-storage")),
            ),
            (BTreeMap::from([(0xa8, Ok(0x204))]), Ok(None)),
        ] {
            let [group] = groups(
                &member.fields,
                &member.paths,
                &|_| Some((START, member.code.as_slice())),
                &BTreeMap::from([(0x2b0, 1), (0x2b1, 0), (0x2b2, 0), (0x2b3, 0)]),
                execute_bindings("0xa8", "0x2b0"),
                &Ok(Countdown { counts: 0x40 }),
                &storage,
            )
            .groups
            .try_into()
            .unwrap();
            assert_eq!(group.units[0].factor, expected);
            assert_eq!(group.combination.is_ok(), expected.is_ok());
        }
    }
}

#[test]
fn scoped_byte_bounds_require_the_subtype_selection_and_literal_width() {
    use crate::engine::analysis::numeric::{NumericFacts, NumericReader};
    use crate::engine::analysis::scoped_numeric::{Facts, Layout, Shared, Subtype};
    use crate::{GrammarProperty, NumericConversion};
    let mut facts = Facts {
        shared: Shared {
            forms: Err(Unresolved::new("unused-forms")),
            literal_preserves_references: Err(Unresolved::new("unused-selection")),
            selection: Ok(Layout {
                literal: 0x28,
                location: 8,
                variable: 0,
                trigger: 0x10,
                script_value: 0x18,
                modifier: 0x20,
                modifier_unset: 0,
            }),
        },
        subtypes: [(
            0x9000,
            Ok(Subtype::Numeric {
                literal: 0x28,
                token_reader: "numeric-token-reader".into(),
            }),
        )]
        .into(),
    };
    let mut numeric = NumericFacts {
        modifier_entry: Err(Unresolved::new("unused-modifier")),
        readers: Default::default(),
        token_readers: [(
            "numeric-token-reader".into(),
            NumericReader {
                conversion: GrammarProperty::Partial(Some(NumericConversion {
                    width_bits: GrammarProperty::Known(32),
                    ..Default::default()
                })),
                gaps: vec![],
            },
        )]
        .into(),
    };
    assert_eq!(scoped_storage(&facts, &numeric)[&0x9000], Ok(0x2c));
    numeric
        .token_readers
        .get_mut("numeric-token-reader")
        .unwrap()
        .conversion = GrammarProperty::Partial(Some(NumericConversion {
        width_bits: GrammarProperty::Known(64),
        ..Default::default()
    }));
    assert_eq!(scoped_storage(&facts, &numeric)[&0x9000], Ok(0x30));
    let proof = numeric.token_readers.clone();
    numeric.token_readers.clear();
    assert_eq!(
        scoped_storage(&facts, &numeric)[&0x9000],
        Err(Unresolved::new("duration-byte-storage"))
    );
    numeric.token_readers = proof;
    facts.shared.selection.as_mut().unwrap().literal = 0x30;
    assert_eq!(
        scoped_storage(&facts, &numeric)[&0x9000],
        Err(Unresolved::new("duration-byte-storage"))
    );
    facts.shared.selection = Err(Unresolved::new("missing-selection"));
    assert_eq!(
        scoped_storage(&facts, &numeric)[&0x9000],
        Err(Unresolved::new("duration-byte-storage"))
    );
    facts
        .subtypes
        .insert(0x9000, Err(Unresolved::new("missing-subtype")));
    assert_eq!(
        scoped_storage(&facts, &numeric)[&0x9000],
        Err(Unresolved::new("duration-byte-storage"))
    );
}
