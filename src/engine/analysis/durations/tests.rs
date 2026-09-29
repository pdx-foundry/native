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

    fn groups(&self, execute: Option<Result<Bindings, Unresolved>>) -> Vec<Group> {
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
        )
    }
}

fn execute_bindings(operand: &str, factor: &str) -> Option<Result<Bindings, Unresolved>> {
    Some(Ok([
        ("operand".to_string(), operand.to_string()),
        ("factor".to_string(), factor.to_string()),
    ]
    .into()))
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

#[test]
fn a_stack_temporary_read_forms_no_group() {
    let mut code = Arm64::at(START);
    arm64!(code;
        mov x19, x0;
        add x1, sp, #0x1c0;
        bl extern 0x9000;
        ldr w8, [sp, #0x1c0];
        lsl w9, w8, #5;
        sub w8, w9, w8, lsl #1;
        str w8, [x19, #0x2d8];
        ret
    );
    let mut member = Member::new(code).key("months", 0x1004, 0x1008, READ_INT, false);
    member.fields[0].readers[0] = ReaderJoin::Missing(Unresolved::new("reader-routing"));

    assert!(member.groups(None).is_empty());
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
    )
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
    )
    .try_into()
    .unwrap();

    assert!(group.combination.is_ok());
    assert_eq!(
        group.consumption,
        Err(Unresolved::new("duration-flag-update"))
    );
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
    assert_eq!(bindings["operand"], "0xa8");
    assert_eq!(bindings["factor"], "0x2b0");

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
    assert_eq!(other_slot["factor"], "0x2b8");

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
