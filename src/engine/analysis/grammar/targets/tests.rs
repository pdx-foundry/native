use super::*;
use crate::engine::analysis::{
    assembler::{Arm64, arm64},
    declarations::Function,
};

fn accessor() -> GrammarInput {
    let mut empty = Arm64::at(0x3000);
    arm64!(empty; ret);
    let mut other = Arm64::at(0x4000);
    arm64!(other; ret);
    let mut input = super::super::tests::input(empty, other);
    let mut body = Arm64::at(0x8000);
    arm64!(body; ldr x8, [x0, #8]; cmp x8, #4; b.ne >rejected; ldr x0, [x0, #24]; ret; rejected:);
    body.address(8, 0x20000);
    arm64!(body; ldr x0, [x8]; ret);
    input.declarations.functions.insert(
        0x8000,
        Function {
            address: 0x8000,
            code: body.bytes(),
        },
    );
    input.declarations.scope_names = Some(vec![
        "none".into(),
        "any".into(),
        "country".into(),
        "ship".into(),
    ]);
    input.targets.scope_type_offset = 8;
    input.targets.scope_object_offset = 24;
    input
        .targets
        .nulls
        .insert(0x8000, AccessorNullObject::Global(0x20000));
    input.command_bindings.scope_accessors = input.targets.nulls.clone();
    input
}

#[test]
fn accessor_accepts_only_the_compared_bit() {
    let input = accessor();
    assert_eq!(getter_table(&input)[&0x8000].mask(), Ok(4));
}

#[test]
fn no_null_object_is_unresolved() {
    let mut input = accessor();
    input
        .targets
        .nulls
        .insert(0x8000, AccessorNullObject::NoNullObject);
    assert_eq!(getter_table(&input)[&0x8000].mask(), Err("no null object"));
}

#[test]
fn changed_type_offset_does_not_guess_a_type() {
    let mut input = accessor();
    input.targets.scope_type_offset = 16;
    assert!(getter_table(&input)[&0x8000].mask().is_err());
}
fn put(input: &mut GrammarInput, body: Arm64) {
    input.declarations.functions.insert(
        body.start(),
        Function {
            address: body.start(),
            code: body.bytes(),
        },
    );
}

fn scenario(validation: Arm64, role: Arm64) -> GrammarInput {
    let mut input = accessor();
    let mut getter = Arm64::at(0x8100);
    arm64!(getter; sub sp, sp, #64; mov x8, sp; bl extern 0x8200; mov x0, sp; bl extern 0x8000; add sp, sp, #64; ret);
    let mut read = Arm64::at(0x2000);
    arm64!(read; mov x19, x0; sub sp, sp, #64; add x1, x1, #0x278;
        mov x0, sp; bl extern 0xc000; mov x1, sp; add x0, sp, #16; bl extern 0xc100;
        add x0, x19, #0x40; add x1, sp, #16; bl extern 0xc200; add sp, sp, #64; mov w0, #1; ret);
    let mut init = Arm64::at(0x6200);
    arm64!(init; ret);
    for body in [getter, read, init, validation, role] {
        put(&mut input, body);
    }
    input
        .targets
        .nulls
        .insert(0x8100, AccessorNullObject::Global(0x20000));
    input.command_bindings.target_getters.insert(0x8100);
    input.command_bindings.target_resolver = 0x8200;
    input.command_bindings.target_scope_type = 0x8300;
    input.command_bindings.token_copy.insert(0xc000);
    input.command_bindings.target_from_token.insert(0xc100);
    input.command_bindings.target_move = 0xc200;
    input.command_bindings.validation_slot = 0x98;
    input.command_bindings.reader_value_token_offset = 0x278;
    input.command_bindings.error_logs.insert(0x8400);
    input.forms.target_size = 16;
    input.forms.role_slot = 0x50;
    input
        .forms
        .initializers
        .insert(0x6200, super::super::forms::Initializer::NoLookup);
    input.declarations.pointers.extend([
        (0x11000 + 0x90, 0x6200),
        (0x11000 + 0x98, 0x6300),
        (0x11000 + 0x50, 0x6400),
    ]);
    input
}
fn validation() -> Arm64 {
    let mut code = Arm64::at(0x6300);
    arm64!(code; mov w0, #1; ret);
    code
}
fn role() -> Arm64 {
    let mut code = Arm64::at(0x6400);
    arm64!(code; add x0, x0, #0x40; bl extern 0x8100; ret);
    code
}
fn reader() -> crate::engine::analysis::declarations::CommandReader {
    crate::engine::analysis::declarations::CommandReader {
        vtable: 0x11000,
        read: 0x2000,
        member: 0x3000,
    }
}
fn checks(input: &GrammarInput) -> [Check; 3] {
    let [read, validate] =
        super::super::forms::target_checks(input, reader(), &BTreeMap::new(), 0x40, None);
    [
        read,
        validate,
        execution(
            input,
            reader(),
            &BTreeMap::new(),
            0x40,
            &BTreeSet::from([0x40]),
        ),
    ]
}

#[test]
fn whole_execution_path_resolves_the_stored_argument() {
    let input = scenario(validation(), role());
    assert_eq!(
        combine(&checks(&input)),
        Ok((4, crate::TargetCheckStage::Execution))
    );
}

#[test]
fn validation_with_diagnostics_establishes_the_earliest_stage() {
    let mut validation = Arm64::at(0x6300);
    arm64!(validation; add x0, x0, #0x40; bl extern 0x8300; cmp x0, #4; b.eq >yes; bl extern 0x8400; mov w0, #0; ret; yes:; mov w0, #1; ret);
    let input = scenario(validation, role());
    assert_eq!(
        combine(&checks(&input)),
        Ok((4, crate::TargetCheckStage::Validation))
    );
}

#[test]
fn validation_false_without_log_blocks_an_established_execution_check() {
    let mut validation = Arm64::at(0x6300);
    arm64!(validation; add x0, x0, #0x40; bl extern 0x8300; cmp x0, #4; cset w0, eq; ret);
    let input = scenario(validation, role());
    assert_eq!(checks(&input)[2], Check::Established(4));
    assert!(combine(&checks(&input)).is_err());
}

#[test]
fn unchecked_target_never_becomes_any() {
    let mut role = Arm64::at(0x6400);
    arm64!(role; ret);
    assert_eq!(
        combine(&checks(&scenario(validation(), role))),
        Err("target not checked")
    );
}

#[test]
fn loads_and_unknown_calls_do_not_establish_a_scope_set() {
    for call in [false, true] {
        let mut role = Arm64::at(0x6400);
        if call {
            arm64!(role; add x0, x0, #0x40; bl extern 0x8500; ret);
        } else {
            arm64!(role; ldr x0, [x0, #0x40]; ret);
        }
        assert!(matches!(
            checks(&scenario(validation(), role))[2],
            Check::Unresolved(_)
        ));
    }
}

#[test]
fn later_checks_must_contain_the_earliest_set() {
    use crate::TargetCheckStage::Validation;
    assert_eq!(
        combine(&[
            Check::Absent,
            Check::Established(4),
            Check::Established(u64::MAX)
        ]),
        Ok((4, Validation))
    );
    assert_eq!(
        combine(&[Check::Absent, Check::Established(12), Check::Established(8)]),
        Err("stage type sets differ")
    );
    assert_eq!(
        combine(&[Check::Absent, Check::Absent, Check::Established(0)]),
        Err("zero type set")
    );
}

#[test]
fn unfinished_getters_and_unknown_results_are_not_acceptance() {
    for indirect in [false, true] {
        let mut input = accessor();
        let mut body = Arm64::at(0x8000);
        if indirect {
            arm64!(body; blr x9; ret);
        } else {
            arm64!(body; ldr x0, [x0, #48]; ret);
        }
        put(&mut input, body);
        assert!(getter_table(&input)[&0x8000].mask().is_err());
    }
}

#[test]
fn a_getter_on_another_object_does_not_check_the_target() {
    let mut role = Arm64::at(0x6400);
    arm64!(role; mov x0, sp; bl extern 0x8100; ret);
    assert!(combine(&checks(&scenario(validation(), role))).is_err());
}

#[test]
fn a_stop_after_a_getter_does_not_establish_execution() {
    let mut role = Arm64::at(0x6400);
    arm64!(role; add x0, x0, #0x40; bl extern 0x8100; br x9);
    assert!(matches!(
        checks(&scenario(validation(), role))[2],
        Check::Unresolved(_)
    ));
}

#[test]
fn different_getters_on_alternative_paths_are_unresolved() {
    let mut role = Arm64::at(0x6400);
    arm64!(role; ldr x8, [x1]; cbz x8, >other; add x0, x0, #0x40; b extern 0x8100;
        other:; add x0, x0, #0x40; b extern 0x9100);
    let mut input = scenario(validation(), role);
    let mut accessor = Arm64::at(0x9000);
    arm64!(accessor; ldr x8, [x0, #8]; cmp x8, #8; b.ne >no; ldr x0, [x0, #24]; ret; no:);
    accessor.address(8, 0x20000);
    arm64!(accessor; ldr x0, [x8]; ret);
    let mut getter = Arm64::at(0x9100);
    arm64!(getter; sub sp, sp, #64; mov x8, sp; bl extern 0x8200; mov x0, sp; bl extern 0x9000; add sp, sp, #64; ret);
    for body in [accessor, getter] {
        put(&mut input, body);
    }
    input.targets.nulls.extend([
        (0x9000, AccessorNullObject::Global(0x20000)),
        (0x9100, AccessorNullObject::Global(0x20000)),
    ]);
    input
        .command_bindings
        .scope_accessors
        .insert(0x9000, AccessorNullObject::Global(0x20000));
    input.command_bindings.target_getters.insert(0x9100);
    assert_eq!(checks(&input)[2], Check::Unresolved("different type sets"));
}

#[test]
fn no_null_object_on_a_getter_route_prevents_a_scope_proof() {
    let mut input = scenario(validation(), role());
    input
        .targets
        .nulls
        .insert(0x8000, AccessorNullObject::NoNullObject);
    assert_eq!(getter_table(&input)[&0x8100].mask(), Err("no null object"));
}

#[test]
fn nested_target_keeps_its_proved_offset_and_named_path() {
    let mut input = super::super::tests::nested_input();
    let mut execution = Arm64::at(0x6400);
    arm64!(execution; add x0, x0, #0x60; b extern 0x8100);
    let target_input = scenario(validation(), execution);
    for address in [0x6200, 0x6300, 0x6400, 0x8000, 0x8100] {
        input.declarations.functions.insert(
            address,
            target_input.declarations.functions[&address].clone(),
        );
    }
    input.targets = target_input.targets;
    input.forms = target_input.forms;
    input.command_bindings = target_input.command_bindings;
    input.declarations.scope_names = target_input.declarations.scope_names;
    input
        .declarations
        .pointers
        .extend([(0x11090, 0x6200), (0x11098, 0x6300), (0x11050, 0x6400)]);
    input.key_readers.value_token = 0x278;
    input.key_readers.token_copy = vec![0xc000];
    input.key_readers.target_construct = vec![0xc100];
    input.key_readers.target_move = Some(0xc200);
    input.key_readers.compound_sizes[0] = 16;
    let mut read = Arm64::at(0x2000);
    arm64!(read; b extern 0x3000);
    put(&mut input, read);
    let mut member = Arm64::at(0x4000);
    arm64!(member; cmp w2, #8; b.ne >reject; mov x19, x0; sub sp, sp, #64;
        add x1, x1, #0x278; mov x0, sp; bl extern 0xc000;
        mov x1, sp; add x0, sp, #16; bl extern 0xc100;
        add x0, x19, #0x40; add x1, sp, #16; bl extern 0xc200;
        add sp, sp, #64; mov w0, #1; ret;
        reject:; mov x0, x1; b extern 0xa000);
    put(&mut input, member);
    let grammar = super::super::analyze(&input, 0x10000).unwrap();
    assert_eq!(grammar.targets.len(), 1);
    assert_eq!(
        grammar.targets[0].path,
        crate::ArgumentPath::Key(vec!["parent".into(), "child".into()])
    );
    assert_eq!(
        grammar.targets[0].stage,
        crate::TargetCheckStage::Execution,
        "{:?}",
        grammar.targets
    );
}

fn country_validation() -> Arm64 {
    let mut body = Arm64::at(0x6300);
    arm64!(body; add x0, x0, #0x40; bl extern 0x8300; cmp x0, #4; b.eq >yes;
        bl extern 0x8400; mov w0, #0; ret; yes:; mov w0, #1; ret);
    body
}

#[test]
fn a_later_generic_resolver_preserves_the_validation_set() {
    let mut role = Arm64::at(0x6400);
    arm64!(role; sub sp, sp, #64; mov x8, sp; add x0, x0, #0x40; bl extern 0x8200; add sp, sp, #64; ret);
    let input = scenario(country_validation(), role);
    assert_eq!(
        combine(&checks(&input)),
        Ok((4, crate::TargetCheckStage::Validation))
    );
}

#[test]
fn later_narrowing_does_not_get_reported_as_a_validation_intersection() {
    let mut validation = Arm64::at(0x6300);
    arm64!(validation; add x0, x0, #0x40; bl extern 0x8300; cmp x0, #4; b.eq >yes;
        cmp x0, #8; b.eq >yes; bl extern 0x8400; mov w0, #0; ret; yes:; mov w0, #1; ret);
    let input = scenario(validation, role());
    assert_eq!(combine(&checks(&input)), Err("stage type sets differ"));
}

#[test]
fn execution_facts_require_only_influential_receiver_state() {
    for both in [false, true] {
        let mut role = Arm64::at(0x6400);
        arm64!(role; ldrb w8, [x0, #0x30]; cbz w8, >other; add x0, x0, #0x40; b extern 0x8100; other:);
        if both {
            arm64!(role; add x0, x0, #0x40; b extern 0x8100);
        } else {
            arm64!(role; ret);
        }
        let input = scenario(validation(), role);
        assert_eq!(
            checks(&input)[2],
            if both {
                Check::Established(4)
            } else {
                Check::Unresolved("receiver-state")
            }
        );
        assert_eq!(
            execution(
                &input,
                reader(),
                &BTreeMap::from([(0x30, 1)]),
                0x40,
                &BTreeSet::from([0x40])
            ),
            Check::Established(4)
        );
    }
}

#[test]
fn a_check_in_the_storing_reader_is_while_reading() {
    let mut input = scenario(validation(), role());
    let mut read = Arm64::at(0x2000);
    arm64!(read; mov x19, x0; sub sp, sp, #64; add x1, x1, #0x278;
        mov x0, sp; bl extern 0xc000; mov x1, sp; add x0, sp, #16; bl extern 0xc100;
        add x0, x19, #0x40; add x1, sp, #16; bl extern 0xc200;
        add x0, x19, #0x40; bl extern 0x8300; cmp x0, #4; b.eq >yes; bl extern 0x8400;
        yes:; add sp, sp, #64; mov w0, #1; ret);
    put(&mut input, read);
    assert_eq!(
        combine(&checks(&input)),
        Ok((4, crate::TargetCheckStage::WhileReading))
    );
}

#[test]
fn a_resolver_does_not_hide_unclassified_uses_of_its_scope() {
    for call in [false, true] {
        let mut role = Arm64::at(0x6400);
        arm64!(role; sub sp, sp, #64; mov x8, sp; add x0, x0, #0x40; bl extern 0x8200);
        if call {
            arm64!(role; mov x0, sp; bl extern 0x8500);
        } else {
            arm64!(role; ldr x8, [sp, #8]);
        }
        arm64!(role; add sp, sp, #64; ret);
        assert!(matches!(
            checks(&scenario(validation(), role))[2],
            Check::Unresolved(_)
        ));
    }
}

#[test]
fn getter_input_type_is_not_the_calling_context_type() {
    let mut input = scenario(validation(), role());
    let mut getter = Arm64::at(0x8100);
    arm64!(getter; ldr x0, [x1, #24]; ret);
    put(&mut input, getter);
    assert_eq!(getter_table(&input)[&0x8100].mask(), Err("unknown result"));
}

#[test]
fn resolving_another_target_does_not_use_the_probed_input_bit() {
    let mut input = scenario(validation(), role());
    let mut getter = Arm64::at(0x8100);
    arm64!(getter; mov x0, x1; mov x8, sp; bl extern 0x8200; ret);
    put(&mut input, getter);
    assert_eq!(
        getter_table(&input)[&0x8100].mask(),
        Err("resolver received another target")
    );
}

#[test]
fn cut_getter_body_cannot_establish_a_scope_set() {
    let mut input = accessor();
    input.forms.cut_bodies.insert(0x8000);
    assert_eq!(getter_table(&input)[&0x8000].mask(), Err("cut getter body"));
}

#[test]
fn reader_helper_with_a_saved_owner_cannot_prove_target_absence() {
    let mut input = scenario(validation(), role());
    let read = input.declarations.functions.get_mut(&0x2000).unwrap();
    let mut helper = Arm64::at(read.address + read.code.len() as u64 - 4);
    arm64!(helper; sub sp, sp, #16; str x19, [sp]; mov x0, sp;
        bl extern 0x8500; add sp, sp, #16; mov w0, #1; ret);
    read.code.truncate(read.code.len() - 4);
    read.code.extend(helper.bytes());
    assert_eq!(
        checks(&input)[0],
        Check::Unresolved("target chain unfinished")
    );
}

fn two_getters(role: Arm64) -> GrammarInput {
    let mut input = scenario(validation(), role);
    let mut accessor = Arm64::at(0x9000);
    arm64!(accessor; ldr x8, [x0, #8]; cmp x8, #8; b.ne >no;
        ldr x0, [x0, #24]; ret; no:);
    accessor.address(8, 0x20000);
    arm64!(accessor; ldr x0, [x8]; ret);
    let mut getter = Arm64::at(0x9100);
    arm64!(getter; sub sp, sp, #64; mov x8, sp; bl extern 0x8200;
        mov x0, sp; bl extern 0x9000; add sp, sp, #64; ret);
    for body in [accessor, getter] {
        put(&mut input, body);
    }
    input.targets.nulls.extend([
        (0x9000, AccessorNullObject::Global(0x20000)),
        (0x9100, AccessorNullObject::Global(0x20000)),
    ]);
    input
        .command_bindings
        .scope_accessors
        .insert(0x9000, AccessorNullObject::Global(0x20000));
    input.command_bindings.target_getters.insert(0x9100);
    input
}

#[test]
fn distinct_target_getters_establish_each_arguments_own_set() {
    let mut role = Arm64::at(0x6400);
    arm64!(role; mov x19, x0; add x0, x19, #0x40; bl extern 0x8100;
        add x0, x19, #0x60; bl extern 0x9100; ret);
    let input = two_getters(role);
    for (offset, mask) in [(0x40, 4), (0x60, 8)] {
        let check = execution(
            &input,
            reader(),
            &BTreeMap::new(),
            offset,
            &BTreeSet::from([0x40, 0x60]),
        );
        assert_eq!(
            combine(&[Check::Absent, Check::Absent, check]),
            Ok((mask, crate::TargetCheckStage::Execution))
        );
    }
}

#[test]
fn a_second_getter_on_the_current_target_is_still_a_constraint() {
    let mut role = Arm64::at(0x6400);
    arm64!(role; mov x19, x0; add x0, x19, #0x40; bl extern 0x8100;
        add x0, x19, #0x40; bl extern 0x9100; ret);
    let input = two_getters(role);
    assert_eq!(
        execution(
            &input,
            reader(),
            &BTreeMap::new(),
            0x40,
            &BTreeSet::from([0x40, 0x60])
        ),
        Check::Unresolved("different type sets")
    );
}

#[test]
fn only_exact_disjoint_collected_targets_are_unrelated() {
    for (address, collected, callee) in [
        (0, 0x60, 0x9100),
        (0x61, 0x60, 0x9100),
        (0x48, 0x48, 0x9100),
        (0x60, 0x60, 0x8500),
    ] {
        let mut role = Arm64::at(0x6400);
        arm64!(role; mov x19, x0; add x0, x19, #0x40; bl extern 0x8100);
        arm64!(role; add x0, x19, #address);
        role.call(callee);
        arm64!(role; ret);
        let input = two_getters(role);
        assert_eq!(
            execution(
                &input,
                reader(),
                &BTreeMap::new(),
                0x40,
                &BTreeSet::from([0x40, collected])
            ),
            Check::Unresolved("unclassified execution call")
        );
    }
}

#[test]
fn another_resolvers_accessors_do_not_check_this_target() {
    let mut role = Arm64::at(0x6400);
    arm64!(role; mov x19, x0; sub sp, sp, #64;
        add x0, x19, #0x60; mov x8, sp; bl extern 0x8200;
        mov x0, sp; bl extern 0x9000;
        add x0, x19, #0x40; bl extern 0x8100; add sp, sp, #64; ret);
    let input = two_getters(role);
    assert_eq!(
        execution(
            &input,
            reader(),
            &BTreeMap::new(),
            0x40,
            &BTreeSet::from([0x40, 0x60])
        ),
        Check::Established(4)
    );
}

#[test]
fn rejected_and_unresolved_value_targets_are_not_collected() {
    use crate::GrammarProperty;
    for diagnosed in [true, false] {
        let mut validation = Arm64::at(0x6300);
        if diagnosed {
            arm64!(validation; bl extern 0x8400);
        }
        arm64!(validation; mov w0, #0; ret);
        let input = scenario(validation, role());
        let result = super::super::analyze(&input, 0x10000).unwrap();
        assert!(result.targets.is_empty());
        let answer = crate::session::grammar::normalize(
            Ok(&result),
            "example",
            crate::BuildId("authored".into()),
            &Default::default(),
        );
        if diagnosed {
            assert_eq!(answer.value.forms, GrammarProperty::Known(vec![]));
            assert_eq!(answer.value.targets, GrammarProperty::Known(vec![]));
        } else {
            assert!(matches!(answer.value.forms, GrammarProperty::Partial(_)));
            assert!(!matches!(answer.value.targets, GrammarProperty::Known(_)));
            assert!(
                answer
                    .gaps
                    .iter()
                    .any(|gap| gap.detail == "target-arguments")
            );
        }
    }
}
