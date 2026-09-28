use super::*;
use crate::engine::analysis::{
    assembler::{Arm64, arm64},
    declarations::Function,
};

const READ: u64 = 0x2000;
const MEMBER: u64 = 0x3000;
const ASSIGN_BODY: u64 = 0x6100;
const INIT: u64 = 0x6200;
const VALIDATE: u64 = 0x6300;
const STRING: u64 = 0x7000;
const LOG: u64 = 0x8000;
const VTABLE: u64 = 0x11000;

fn returning(at: u64) -> Arm64 {
    let mut body = Arm64::at(at);
    arm64!(body; ret);
    body
}
fn true_validation() -> Arm64 {
    let mut body = Arm64::at(VALIDATE);
    arm64!(body; mov w0, #1; ret);
    body
}
fn input(read: Arm64, assign: Arm64, validate: Arm64) -> GrammarInput {
    let mut input = super::super::tests::input(returning(MEMBER), returning(0x4000));
    for body in [read, assign, returning(INIT), validate] {
        put(&mut input, body);
    }
    input.forms.token_text_offset = 16;
    input.forms.string_size = 32;
    input.forms.target_size = 0x190;
    input.command_bindings.assign_slot = 0x20;
    input.command_bindings.validation_slot = 0x98;
    input.command_bindings.reader_value_token_offset = 0x278;
    input.command_bindings.boolean_tokens = [7, 8];
    input.command_bindings.error_logs.insert(LOG);
    input.declarations.pointers.extend([
        (VTABLE + 0x20, ASSIGN_BODY),
        (VTABLE + 0x90, INIT),
        (VTABLE + 0x98, VALIDATE),
    ]);
    input.forms.initializers.insert(INIT, Initializer::NoLookup);
    input
        .forms
        .shared
        .insert(STRING, "CReader::Read(CString&, bool)".into());
    input
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
fn receiver() -> CommandReader {
    CommandReader {
        vtable: VTABLE,
        read: READ,
        member: MEMBER,
    }
}
fn analyze(input: &GrammarInput) -> Arc<Result> {
    super::analyze(input, receiver(), &BTreeMap::from([(0, 0)])).0
}
fn string_read() -> Arm64 {
    let mut body = Arm64::at(READ);
    arm64!(body; mov x8, x0; mov x0, x1; add x1, x8, #0x40; b extern STRING as usize);
    body
}
fn assign_read() -> Arm64 {
    let mut body = Arm64::at(READ);
    arm64!(body; ldr x8, [x0]; ldr x8, [x8, #0x20]; add x1, x1, #0x278; br x8);
    body
}
fn false_assign() -> Arm64 {
    let mut body = Arm64::at(ASSIGN_BODY);
    arm64!(body; mov w0, #0; ret);
    body
}
fn boolean_assign() -> Arm64 {
    let mut body = Arm64::at(ASSIGN_BODY);
    arm64!(body;
        ldr w8, [x1];
        cmp w8, #7; // first Boolean token
        b.eq >yes;
        cmp w8, #8; // second Boolean token
        b.eq >no;
        strb wzr, [x0, #0x41]; // invalid marker
        mov w0, #0; ret;
        yes:; mov w8, #1; b >store;
        no:; mov w8, #0;
        store:; strb w8, [x0, #0x40];
        mov w8, #1; strb w8, [x0, #0x41];
        mov w0, #1; ret
    );
    body
}
fn boolean_validation() -> Arm64 {
    let mut body = Arm64::at(VALIDATE);
    arm64!(body; ldrb w8, [x0, #0x41]; cbnz w8, >valid; bl extern LOG as usize; mov w0, #0; ret; valid:; mov w0, #1; ret);
    body
}

#[test]
fn shared_value_runs_validation_and_is_accepted() {
    let result = analyze(&input(string_read(), false_assign(), true_validation()));
    assert!(result.complete, "{result:?}");
    assert_eq!(result.alternatives.len(), 1);
    assert!(result.alternatives[0].accepted);
    assert_eq!(result.alternatives[0].value.kind, ReaderKind::String);
}

#[test]
fn validation_false_without_log_is_unresolved_not_rejected() {
    let mut validation = Arm64::at(VALIDATE);
    arm64!(validation; mov w0, #0; ret);
    let result = analyze(&input(string_read(), false_assign(), validation));
    assert!(!result.complete);
    let alternative = &result.alternatives[0];
    assert!(!alternative.accepted);
    assert!(
        alternative
            .paths
            .iter()
            .all(|path| path.class == PathClass::Unresolved)
    );
    assert_eq!(
        alternative.paths[0].stages.last().unwrap().cause,
        Some("false without diagnostic")
    );
}

#[test]
fn false_assign_then_true_base_validation_is_unresolved() {
    let result = analyze(&input(assign_read(), false_assign(), true_validation()));
    assert!(!result.complete);
    assert!(!result.alternatives[0].accepted);
    assert!(
        result.alternatives[0]
            .paths
            .iter()
            .all(|path| path.class == PathClass::Unresolved)
    );
    assert_eq!(
        result.alternatives[0].paths[0].stages[0].stage,
        Stage::Assign
    );
}

#[test]
fn later_log_rejects_even_when_assign_returned_false() {
    let mut validation = Arm64::at(VALIDATE);
    arm64!(validation; bl extern LOG as usize; mov w0, #0; ret);
    let result = analyze(&input(assign_read(), false_assign(), validation));
    assert!(result.complete, "{result:?}");
    assert!(!result.alternatives[0].accepted);
    assert!(
        result.alternatives[0]
            .paths
            .iter()
            .all(|path| path.class == PathClass::Rejecting)
    );
}

#[test]
fn validation_unknown_or_mixed_paths_are_unresolved() {
    for validation in [returning(VALIDATE), {
        let mut body = Arm64::at(VALIDATE);
        arm64!(body; cbz x2, >valid; bl extern LOG as usize; valid:; mov w0, #1; ret);
        body
    }] {
        let result = analyze(&input(string_read(), false_assign(), validation));
        assert!(!result.complete, "{result:?}");
        assert!(!result.alternatives[0].accepted);
    }
}

#[test]
fn diagnostic_rejects_a_true_result() {
    let mut validation = Arm64::at(VALIDATE);
    arm64!(validation; bl extern LOG as usize; mov w0, #1; ret);
    let result = analyze(&input(string_read(), false_assign(), validation));
    assert!(result.complete);
    assert!(!result.alternatives[0].accepted);
}

#[test]
fn boolean_probe_accepts_two_distinct_values_and_rejects_marker() {
    let result = analyze(&input(
        assign_read(),
        boolean_assign(),
        boolean_validation(),
    ));
    assert!(result.complete, "{result:?}");
    let accepted: Vec<_> = result
        .alternatives
        .iter()
        .filter(|value| value.accepted)
        .collect();
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].value.kind, ReaderKind::Boolean);
}

#[test]
fn unresolved_boolean_marker_keeps_forms_partial() {
    let result = analyze(&input(assign_read(), boolean_assign(), true_validation()));
    assert!(!result.complete, "{result:?}");
    assert!(
        result
            .alternatives
            .iter()
            .any(|value| value.accepted && value.value.kind == ReaderKind::Boolean)
    );
}

#[test]
fn a_third_token_that_assign_accepts_keeps_boolean_forms_partial() {
    let mut assign = Arm64::at(ASSIGN_BODY);
    arm64!(assign;
        ldr w8, [x1];
        cmp w8, #7; // first Boolean token
        b.eq >yes;
        cmp w8, #8; // second Boolean token
        b.eq >no;
        cmp w8, #9; // a third token that the marker probe never takes
        b.eq >third;
        strb wzr, [x0, #0x41]; // invalid marker
        mov w0, #0; ret;
        yes:; mov w8, #1; b >store;
        no:; mov w8, #0; b >store;
        third:; mov w8, #2;
        store:; strb w8, [x0, #0x40];
        mov w8, #1; strb w8, [x0, #0x41];
        mov w0, #1; ret
    );
    let result = analyze(&input(assign_read(), assign, boolean_validation()));
    assert!(!result.complete, "{result:?}");
    assert!(
        result
            .stops
            .iter()
            .any(|stop| stop.reason == "form-token-coverage")
    );
}

#[test]
fn an_unentered_helper_that_receives_the_command_is_a_stop() {
    for (passes_command, complete) in [(true, false), (false, true)] {
        let mut read = Arm64::at(READ);
        arm64!(read; mov x19, x0; mov x20, x1; mov x1, #0);
        if !passes_command {
            arm64!(read; mov x0, #0);
        }
        arm64!(read; bl extern 0xa000; mov x0, x20; add x1, x19, #0x40; b extern STRING as usize);
        let result = analyze(&input(read, false_assign(), true_validation()));
        assert_eq!(result.complete, complete, "{:?}", result.stops);
        assert_eq!(
            result
                .stops
                .iter()
                .any(|stop| stop.reason == "form-command-call"),
            passes_command,
            "{:?}",
            result.stops
        );
    }
}

#[test]
fn block_reader_is_a_boundary_without_a_diagnostic_claim() {
    let mut read = Arm64::at(READ);
    arm64!(read; b extern 0x9000);
    let mut input = input(read, false_assign(), true_validation());
    input
        .forms
        .shared
        .insert(0x9000, "CEffect::Read(CReader&, EScopeType)".into());
    let result = analyze(&input);
    assert!(result.complete && result.block, "{result:?}");
    assert!(result.alternatives.is_empty());
}

#[test]
fn wrong_reader_argument_and_unclassified_token_call_are_stops() {
    let mut wrong = Arm64::at(READ);
    arm64!(wrong; mov x0, x2; b extern STRING as usize);
    let mut unknown = Arm64::at(READ);
    arm64!(unknown; add x1, x1, #0x278; b extern 0xa000);
    for read in [wrong, unknown] {
        let result = analyze(&input(read, false_assign(), true_validation()));
        assert!(!result.complete);
        assert!(
            result
                .stops
                .iter()
                .any(|stop| stop.reason == "form-reader-call")
        );
    }
}

#[test]
fn cut_body_is_not_known() {
    let mut input = input(string_read(), false_assign(), true_validation());
    input.forms.cut_bodies.insert(READ);
    let result = analyze(&input);
    assert!(!result.complete);
}

#[test]
fn known_receiver_byte_selects_the_proved_form() {
    let mut read = Arm64::at(READ);
    arm64!(read; ldrb w8, [x0, #0x30]; cbz w8, >skip; ldr x8, [x0]; ldr x8, [x8, #0x20]; add x1, x1, #0x278; br x8; skip:; ret);
    let input = input(read, boolean_assign(), boolean_validation());
    let known = super::analyze(&input, receiver(), &BTreeMap::from([(0x30, 1)])).0;
    assert!(known.complete, "{known:?}");
    let unknown = super::analyze(&input, receiver(), &BTreeMap::new()).0;
    assert!(!unknown.complete);
    assert!(unknown.receiver_state);
    assert!(unknown.alternatives.iter().all(|value| !value.accepted));
}

#[test]
fn equal_facts_on_both_receiver_branches_need_no_state() {
    let mut read = Arm64::at(READ);
    arm64!(read; ldrb w8, [x0, #0x30]; cbz w8, >read; nop; read:; mov x8, x0; mov x0, x1; add x1, x8, #0x40; b extern STRING as usize);
    let result = analyze(&input(read, false_assign(), true_validation()));
    assert!(result.complete, "{result:?}");
    assert!(!result.receiver_state);
}

#[test]
fn cache_uses_slot_functions_and_established_receiver_bytes() {
    let mut input = input(assign_read(), boolean_assign(), boolean_validation());
    let first = analyze(&input);
    let second = analyze(&input);
    assert!(Arc::ptr_eq(&first, &second));
    let mut other = receiver();
    other.vtable += 0x100;
    for (slot, function) in [(0x20, ASSIGN_BODY + 0x1000), (0x90, INIT), (0x98, VALIDATE)] {
        input
            .declarations
            .pointers
            .insert(other.vtable + slot, function);
    }
    let mut other_assign = Arm64::at(ASSIGN_BODY + 0x1000);
    arm64!(other_assign; mov w0, #0; ret);
    put(&mut input, other_assign);
    let different = super::analyze(&input, other, &BTreeMap::new()).0;
    assert_ne!(first.key, different.key);
    assert!(first.alternatives.iter().any(|value| value.accepted));
    assert!(different.alternatives.iter().all(|value| !value.accepted));
}

fn lookup() -> InitializationLookup {
    use crate::engine::analysis::references::{KeyMatch, Lookup, Stage as LookupStage};
    InitializationLookup {
        database: "Database".into(),
        directory: Some("example".into()),
        key_offset: 0x40,
        item_offset: 0x60,
        lookup: Lookup {
            stage: LookupStage::OwnerInitialization,
            key_match: Some(KeyMatch::Equal),
            empty_key_looked_up: Some(true),
            missing_yields_null: Some(true),
        },
    }
}
fn found_validation() -> Arm64 {
    let mut body = Arm64::at(VALIDATE);
    arm64!(body; ldr x8, [x0, #0x60]);
    arm64!(body; movz x9, #0x7000, lsl #32);
    arm64!(body; cmp x8, x9; b.eq >valid; bl extern LOG as usize; mov w0, #0; ret; valid:; mov w0, #1; ret);
    body
}
#[test]
fn inline_lookup_keeps_memory_and_runs_before_validation() {
    let mut input = input(string_read(), false_assign(), found_validation());
    input.forms.initializers.insert(
        INIT,
        Initializer::Lookup {
            lookup: lookup(),
            execution: LookupExecution::Inline {
                always: true,
                null: 0xb000,
            },
        },
    );
    let result = analyze(&input);
    assert!(result.complete, "{result:?}");
    assert!(result.alternatives[0].accepted);
    assert!(!result.alternatives[0].missing.is_empty());
    assert!(
        result.alternatives[0]
            .missing
            .iter()
            .all(|path| path.class == PathClass::Rejecting)
    );
    input.forms.cache.lock().unwrap().clear();
    input.forms.initializers.insert(INIT, Initializer::NoLookup);
    let without_init = analyze(&input);
    assert!(!without_init.alternatives[0].accepted);
}
#[test]
fn conditional_or_unestablished_initializers_are_not_summarized() {
    for initializer in [
        Initializer::Unresolved,
        Initializer::Lookup {
            lookup: lookup(),
            execution: LookupExecution::Inline {
                always: false,
                null: 0xb000,
            },
        },
    ] {
        let mut input = input(string_read(), false_assign(), true_validation());
        input.forms.initializers.insert(INIT, initializer);
        let result = analyze(&input);
        assert!(!result.complete);
        assert_eq!(
            result.alternatives[0].paths[0].stages[1].cause,
            Some("initializer not summarized")
        );
    }
}
#[test]
fn called_lookup_runs_its_caller_and_stores_the_intercepted_result() {
    let mut input = input(string_read(), false_assign(), found_validation());
    let mut init = Arm64::at(INIT);
    arm64!(init; mov x19, x0; add x1, x0, #0x40);
    init.load(0, 0xb000);
    arm64!(init; bl extern 0xb100; str x0, [x19, #0x60]; ret);
    put(&mut input, init);
    input.forms.initializers.insert(
        INIT,
        Initializer::Lookup {
            lookup: lookup(),
            execution: LookupExecution::Getter {
                database: 0xb000,
                getter: 0xb100,
                null: 0xb200,
            },
        },
    );
    let result = analyze(&input);
    assert!(result.complete, "{result:?}");
    assert!(result.alternatives[0].accepted);
}
#[test]
fn unclassified_initializer_call_with_command_stops_the_chain() {
    let mut input = input(string_read(), false_assign(), true_validation());
    let mut init = Arm64::at(INIT);
    arm64!(init; bl extern 0xb100; ret);
    put(&mut input, init);
    let result = analyze(&input);
    assert!(!result.complete);
    assert_eq!(
        result.alternatives[0].paths[0].stages[1].cause,
        Some("form-stage-call")
    );
}
#[test]
fn block_and_value_branches_are_both_kept() {
    let mut read = Arm64::at(READ);
    arm64!(read; ldr w8, [x1, #0x278]; cmp w8, #3; b.eq >block; mov x8, x0; mov x0, x1; add x1, x8, #0x40; b extern STRING as usize; block:; b extern 0x9000);
    let mut input = input(read, false_assign(), true_validation());
    input
        .forms
        .shared
        .insert(0x9000, "CEffect::Read(CReader&, EScopeType)".into());
    let result = analyze(&input);
    assert!(result.complete && result.block, "{result:?}");
    assert!(result.alternatives[0].accepted);
}
#[test]
fn operator_bytes_are_script_input_and_survive_into_assign() {
    let mut read = Arm64::at(READ);
    arm64!(read; mov x19, x0; mov x20, x1; add x0, x0, #0x30; bl extern 0xb000; mov x0, x19; add x1, x20, #0x278; ldr x8, [x0]; ldr x8, [x8, #0x20]; br x8);
    let assign = boolean_assign();
    // The operator's unknown bytes are read but do not become construction-state evidence.
    let mut validate = Arm64::at(VALIDATE);
    arm64!(validate; ldr w8, [x0, #0x30]; cbz w8, >valid; nop; valid:; mov w0, #1; ret);
    let mut input = input(read, assign, validate);
    input.command_bindings.operator_readers = [0xb000, 0xb008];
    let result = analyze(&input);
    assert!(!result.receiver_state, "{result:?}");
    assert!(!result.key.receiver.contains_key(&0x30));
}
#[test]
fn changed_receiver_state_changes_cache_key_and_result() {
    let mut read = Arm64::at(READ);
    arm64!(read; ldrb w8, [x0, #0x30]; cbz w8, >block; mov x8, x0; mov x0, x1; add x1, x8, #0x40; b extern STRING as usize; block:; b extern 0x9000);
    let mut input = input(read, false_assign(), true_validation());
    input
        .forms
        .shared
        .insert(0x9000, "CEffect::Read(CReader&, EScopeType)".into());
    let first = super::analyze(&input, receiver(), &BTreeMap::from([(0x30, 1)])).0;
    let second = super::analyze(&input, receiver(), &BTreeMap::from([(0x30, 0)])).0;
    assert_ne!(first.key, second.key);
    assert!(!first.block && second.block);
}
#[test]
fn unknown_call_cannot_preserve_a_vtable_for_later_assignment() {
    let mut read = Arm64::at(READ);
    arm64!(read; mov x19, x0; mov x20, x1; bl extern 0xb000; mov x0, x19; add x1, x20, #0x278; ldr x8, [x0]; ldr x8, [x8, #0x20]; br x8);
    let result = analyze(&input(read, boolean_assign(), boolean_validation()));
    assert!(!result.complete);
    assert!(
        result
            .alternatives
            .iter()
            .all(|alternative| !alternative.accepted)
    );
}
#[test]
fn unknown_value_reader_stays_partial_for_accepting_and_rejecting_validation() {
    for rejected in [false, true] {
        let mut assign = Arm64::at(ASSIGN_BODY);
        arm64!(assign; add x0, x0, #0x40; bl extern 0xb000; mov w0, #1; ret);
        let mut validation = Arm64::at(VALIDATE);
        if rejected {
            arm64!(validation; bl extern LOG as usize);
        }
        arm64!(validation; mov w0, #1; ret);
        let mut input = input(assign_read(), assign, validation);
        input.command_bindings.variable_assign = 0xb000;
        let result = analyze(&input);
        assert!(!result.complete);
        assert!(!result.alternatives[0].accepted);
        assert!(
            result
                .stops
                .iter()
                .any(|stop| stop.reason == "form-reader-kind")
        );
    }
}

fn target_reader(at: u64, outer: bool) -> Arm64 {
    let mut body = Arm64::at(at);
    arm64!(body; mov x19, x0; sub sp, sp, #64);
    if outer {
        arm64!(body; add x1, x1, #0x278);
    }
    arm64!(body; mov x0, sp; bl extern 0xc000; mov x1, sp; add x0, sp, #16; bl extern 0xc100; add x0, x19, #0x40; add x1, sp, #16; bl extern 0xc200; add sp, sp, #64; mov w0, #1; ret);
    body
}
fn targets(input: &mut GrammarInput) {
    input.command_bindings.token_copy.insert(0xc000);
    input.command_bindings.target_from_token.insert(0xc100);
    input.command_bindings.target_move = 0xc200;
    input.forms.target_size = 16;
}
#[test]
fn direct_and_assigned_targets_both_continue_through_validation() {
    for outer in [true, false] {
        let read = if outer {
            target_reader(READ, true)
        } else {
            assign_read()
        };
        let mut input = input(read, target_reader(ASSIGN_BODY, false), true_validation());
        targets(&mut input);
        let result = analyze(&input);
        assert!(result.complete, "{result:?}");
        assert_eq!(result.alternatives[0].value.kind, ReaderKind::Target);
        assert!(result.alternatives[0].accepted);
        assert_eq!(
            result.alternatives[0].paths[0].stages.last().unwrap().stage,
            Stage::PostValidate
        );
    }
}
#[test]
fn diagnostic_validation_omits_a_target_alternative() {
    let mut validation = Arm64::at(VALIDATE);
    arm64!(validation; bl extern LOG as usize; mov w0, #0; ret);
    let mut input = input(target_reader(READ, true), false_assign(), validation);
    targets(&mut input);
    let result = analyze(&input);
    assert!(result.complete);
    assert!(!result.alternatives[0].accepted);
}
#[test]
fn assign_result_is_kept_when_read_overwrites_the_return_register() {
    let mut read = Arm64::at(READ);
    arm64!(read; add x1, x1, #0x278; bl extern ASSIGN_BODY as usize; mov w0, #1; ret);
    let result = analyze(&input(read, false_assign(), true_validation()));
    assert!(!result.complete);
    assert_eq!(
        result.alternatives[0].paths[0].stages[0].returned,
        Some(false)
    );
}

#[test]
fn assignment_reference_requires_the_lookup_destination_and_a_tail_store() {
    for (offset, tail, joined) in [
        (0x40, true, true),
        (0x48, true, false),
        (0x40, false, false),
    ] {
        let mut assign = Arm64::at(ASSIGN_BODY);
        arm64!(assign; add x0, x0, #offset; add x1, x1, #16);
        if tail {
            arm64!(assign; b extern 0xc000);
        } else {
            arm64!(assign; bl extern 0xc000; mov w0, #1; ret);
        }
        let mut input = input(assign_read(), assign, true_validation());
        input.forms.token_text_offset = 16;
        input.forms.string_size = 24;
        input.forms.string_copies.insert(0xc000);
        input.forms.initializers.insert(
            INIT,
            Initializer::Lookup {
                lookup: lookup(),
                execution: LookupExecution::Inline {
                    always: true,
                    null: 0xb000,
                },
            },
        );
        let result = analyze(&input);
        assert_eq!(
            result
                .alternatives
                .iter()
                .any(|alternative| alternative.value.initialization == Some(lookup())),
            joined,
            "{result:?}"
        );
    }
}

#[test]
fn a_stopped_stage_cannot_claim_a_later_diagnostic() {
    let mut assign = Arm64::at(ASSIGN_BODY);
    arm64!(assign; strb wzr, [x0, #0x41]; bl extern 0xc000; mov w0, #1; ret);
    let result = analyze(&input(assign_read(), assign, boolean_validation()));
    assert!(!result.complete);
    assert!(
        result
            .alternatives
            .iter()
            .flat_map(|alternative| &alternative.paths)
            .all(|path| path.class == PathClass::Unresolved)
    );
}

#[test]
fn a_block_on_only_one_unknown_receiver_branch_is_not_established() {
    let mut read = Arm64::at(READ);
    arm64!(read; ldrb w8, [x0, #0x30]; cbz w8, >empty; b extern MEMBER as usize; empty:; ret);
    let result = analyze(&input(read, false_assign(), true_validation()));
    assert!(result.receiver_state);
    assert!(!result.complete);
    assert!(!result.block);
}

#[test]
fn dynamic_name_from_the_token_text_is_a_string_alternative() {
    let mut assign = Arm64::at(ASSIGN_BODY);
    arm64!(assign;
        mov x19, x0; ldr x1, [x1, #16]; sub sp, sp, #64; mov x0, sp;
        bl extern 0xc000; mov x0, sp; add x1, x19, #0x40; add x2, x19, #0x80;
        bl extern 0xc100; mov w0, #1; add sp, sp, #64; ret
    );
    let mut input = input(assign_read(), assign, true_validation());
    input.forms.strings_from_text.insert(0xc000);
    input.forms.dynamic_name = 0xc100;
    let result = analyze(&input);
    assert!(result.complete, "{result:?}");
    assert!(result.alternatives[0].accepted);
    assert_eq!(result.alternatives[0].value.kind, ReaderKind::String);
}

#[test]
fn deferred_reference_runs_found_and_bound_null_results() {
    let mut read = Arm64::at(READ);
    arm64!(read; add x2, x0, #0x40; b extern STRING as usize);
    let mut validation = Arm64::at(VALIDATE);
    arm64!(validation; ldr x8, [x0, #0x40]);
    arm64!(validation; mov x9, #0xb000; ldr x9, [x9]; cmp x8, x9; b.ne >found; bl extern LOG as usize; mov w0, #0; ret; found:; mov w0, #1; ret);
    let mut input = input(read, false_assign(), validation);
    input.forms.shared.insert(STRING, "void NParserUtil::ReadKeyReferenceDeferred<CDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CDatabase::ValueType const**)".into());
    input.forms.qualified_references.insert(STRING, 0xb000);
    let result = analyze(&input);
    assert!(result.complete, "{result:?}");
    let alternative = &result.alternatives[0];
    assert!(alternative.accepted);
    assert_eq!(alternative.value.kind, ReaderKind::Reference);
    assert!(!alternative.missing.is_empty());
    assert!(
        alternative
            .missing
            .iter()
            .all(|path| path.class == PathClass::Rejecting)
    );
}

#[test]
fn cache_includes_virtual_helpers_outside_the_fixed_slots() {
    let mut read = Arm64::at(READ);
    arm64!(read; mov x19, x30; ldr x8, [x0]; ldr x8, [x8, #0xa0]; mov x9, x0; mov x0, x1; add x1, x9, #0x40; blr x8; mov x30, x19; ret);
    let mut input = input(read, false_assign(), true_validation());
    let helpers = [STRING, 0x7100, STRING];
    input
        .forms
        .shared
        .insert(0x7100, "CReader::Read(bool&)".into());
    for (index, helper) in helpers.into_iter().enumerate() {
        let vtable = VTABLE + index as u64 * 0x100;
        for (slot, function) in [
            (0x20, ASSIGN_BODY),
            (0x90, INIT),
            (0x98, VALIDATE),
            (0xa0, helper),
        ] {
            input.declarations.pointers.insert(vtable + slot, function);
        }
    }
    let results: Vec<_> = (0..3)
        .map(|index| {
            let reader = CommandReader {
                vtable: VTABLE + index * 0x100,
                ..receiver()
            };
            super::analyze(&input, reader, &BTreeMap::new())
        })
        .collect();
    assert_eq!(results[0].1.slots, results[1].1.slots);
    assert_ne!(results[0].1.functions, results[1].1.functions);
    assert!(!Arc::ptr_eq(&results[0].0, &results[1].0));
    assert_eq!(results[0].0.alternatives[0].value.kind, ReaderKind::String);
    assert_eq!(results[1].0.alternatives[0].value.kind, ReaderKind::Boolean);
    assert_eq!(results[0].1, results[2].1);
    assert!(Arc::ptr_eq(&results[0].0, &results[2].0));
}

#[test]
fn script_kind_forks_do_not_make_equal_receiver_sides_dependent() {
    let mut read = Arm64::at(READ);
    arm64!(read;
        ldrb w8, [x0, #0x30]; cbz w8, >kind; nop;
        kind:; ldr w8, [x1]; cbz w8, >block;
        mov x8, x0; mov x0, x1; add x1, x8, #0x40; b extern STRING as usize;
        block:; b extern MEMBER as usize
    );
    let result = analyze(&input(read, false_assign(), true_validation()));
    assert!(result.complete, "{result:?}");
    assert!(!result.receiver_state);
    assert!(result.block);
    assert!(
        result
            .alternatives
            .iter()
            .any(|a| a.accepted && a.value.kind == ReaderKind::String)
    );
}

fn operator_read() -> Arm64 {
    let mut read = Arm64::at(READ);
    arm64!(read;
        mov x19, x0; mov x20, x1; mov x21, x30;
        add x0, x0, #0x64; bl extern 0x9000;
        mov x0, x19; add x1, x20, #0x278; mov x30, x21;
        b extern ASSIGN_BODY as usize
    );
    read
}

fn invertible_assign(same: bool) -> Arm64 {
    let mut assign = Arm64::at(ASSIGN_BODY);
    arm64!(assign;
        ldr w8, [x1]; cmp w8, #7; b.eq >yes;
        cmp w8, #8; b.eq >no;
        strb wzr, [x0, #0x41]; mov w0, #0; ret;
        yes:; mov w8, #1; b >operator;
        no:
    );
    if same {
        arm64!(assign; mov w8, #1);
    } else {
        arm64!(assign; mov w8, #0);
    }
    arm64!(assign;
        operator:; ldr w9, [x0, #0x64]; cmp w9, #0x427; b.ne >store;
        cbz w8, >one; mov w8, #0; b >store;
        one:; mov w8, #1;
        store:; strb w8, [x0, #0x40];
        mov w8, #1; strb w8, [x0, #0x41]; mov w0, #1; ret
    );
    assign
}

#[test]
fn boolean_probe_pairs_the_same_unknown_operator_decisions() {
    for same in [false, true] {
        let mut input = input(
            operator_read(),
            invertible_assign(same),
            boolean_validation(),
        );
        input.command_bindings.operator_readers[0] = 0x9000;
        let result = analyze(&input);
        let boolean = result
            .alternatives
            .iter()
            .any(|a| a.accepted && a.value.kind == ReaderKind::Boolean);
        assert_eq!(boolean, !same, "{result:?}");
        if !same {
            assert!(result.complete, "{result:?}");
        }
    }
}

#[test]
fn receiver_dependence_suppresses_only_the_affected_value() {
    let mut read = Arm64::at(READ);
    arm64!(read;
        ldr w8, [x1]; cbz w8, >block;
        ldrb w8, [x0, #0x30]; cbz w8, >skip;
        mov x8, x0; mov x0, x1; add x1, x8, #0x40; b extern STRING as usize;
        block:; b extern MEMBER as usize;
        skip:; ret
    );
    let result = analyze(&input(read, false_assign(), true_validation()));
    assert!(!result.complete);
    assert!(result.receiver_state);
    assert!(result.block);
    assert!(result.alternatives.iter().all(|a| !a.accepted));
}

#[test]
fn boolean_probe_requires_a_partner_for_each_operator_decision() {
    let mut assign = Arm64::at(ASSIGN_BODY);
    arm64!(assign;
        ldr w8, [x1]; cmp w8, #7; b.ne >no;
        ldr w9, [x0, #0x64]; cbz w9, >yes; nop;
        yes:; mov w8, #1; b >store;
        no:; mov w8, #0;
        store:; strb w8, [x0, #0x40]; mov w0, #1; ret
    );
    let mut input = input(operator_read(), assign, true_validation());
    input.command_bindings.operator_readers[0] = 0x9000;
    let result = analyze(&input);
    assert!(
        !result
            .alternatives
            .iter()
            .any(|a| a.value.kind == ReaderKind::Boolean)
    );
}

#[test]
fn receiver_origins_follow_stored_bytes_and_compared_flags() {
    let mut read = Arm64::at(READ);
    arm64!(read;
        ldrb w8, [x0, #0x30]; strb w8, [x0, #0x38];
        ldrb w8, [x0, #0x38]; cmp w8, #0; b.eq >skip;
        mov x8, x0; mov x0, x1; add x1, x8, #0x40; b extern STRING as usize;
        skip:; ret
    );
    let result = analyze(&input(read, false_assign(), true_validation()));
    assert!(result.receiver_state, "{result:?}");
    assert!(result.alternatives.iter().all(|a| !a.accepted));
}

#[test]
fn receiver_dependence_does_not_suppress_an_independent_value() {
    let mut read = Arm64::at(READ);
    arm64!(read;
        mov x19, x0; mov x20, x1; mov x21, x30;
        ldrb w8, [x0, #0x30]; cbz w8, >value;
        bl extern MEMBER as usize;
        value:; mov x0, x20; add x1, x19, #0x40; mov x30, x21;
        b extern STRING as usize
    );
    let result = analyze(&input(read, false_assign(), true_validation()));
    assert!(!result.complete);
    assert!(result.receiver_state);
    assert!(!result.block);
    assert!(
        result
            .alternatives
            .iter()
            .any(|a| a.accepted && a.value.kind == ReaderKind::String)
    );
}
