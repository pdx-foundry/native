use super::*;
use crate::BlockFamily;
use crate::engine::analysis::assembler::{Arm64, arm64};
use crate::engine::analysis::decode::decode_arm64;

const NEW: u64 = 0x3000;
const CONSTRUCTOR: u64 = 0x3100;
const LOG: u64 = 0x3200;
const OTHER_CALL: u64 = 0x3300;
const MESSAGE: u64 = 0x8010;
const OTHER_MESSAGE: u64 = 0x8020;

fn rows(code: Arm64) -> Vec<Instruction> {
    let start = code.start();
    decode_arm64(&code.bytes(), start).unwrap()
}

/// A reader that allocates a placeholder and constructs it, as `CEffect::ReadMember` does.
fn constructs_placeholder() -> Vec<Instruction> {
    let mut code = Arm64::at(0x1000);
    arm64!(code; mov w0, #0x108);
    code.call(NEW);
    arm64!(code; mov x25, x0; add x0, sp, #0x40);
    code.call(OTHER_CALL);
    arm64!(code; add x1, sp, #0x40; mov x0, x25);
    code.call(CONSTRUCTOR);
    rows(code)
}

/// A function that constructs the class on its own stack: not a use.
fn constructs_on_stack() -> Vec<Instruction> {
    let mut code = Arm64::at(0x1000);
    arm64!(code; mov w0, #0x108);
    code.call(NEW);
    arm64!(code; mov x25, x0; add x0, sp, #0x40);
    code.call(CONSTRUCTOR);
    rows(code)
}

/// A function that forms `literal` in `x1` and calls `target`.
fn message(literal: u64, target: u64) -> MessageSite {
    let mut code = Arm64::at(0x1000);
    code.address(1, literal);
    arm64!(code; add x0, sp, #0x10);
    code.call(target);
    arm64!(code; ret);
    MessageSite {
        rows: rows(code),
        literal: MESSAGE,
    }
}

/// A function that forms the literal, then overwrites `x1` before the log call.
fn overwritten_message() -> MessageSite {
    let mut code = Arm64::at(0x1000);
    code.address(1, MESSAGE);
    arm64!(code; mov x1, x20);
    code.call(LOG);
    MessageSite {
        rows: rows(code),
        literal: MESSAGE,
    }
}

fn stated() -> StatedForms {
    StatedForms {
        call_forms: vec![CallForm::Value],
        parameter_forms: vec![ParameterForm::Substitution],
    }
}

fn mechanism(uses: Vec<UseCall>) -> MechanismInput {
    MechanismInput {
        mechanism: ExpansionMechanism::ScriptedEffect,
        definitions: vec![DefinitionInput::Directory(Directory::Named(
            "common/scripted_effects".into(),
        ))],
        uses,
        placeholder: true,
        stage: StageInput {
            stage: ExpansionStage::Compile,
            links: vec![
                vec![LinkCaller::Expected, LinkCaller::Allowed],
                vec![LinkCaller::Expected],
            ],
        },
        checks: vec![(ExpansionCheck::UnknownName, message(MESSAGE, LOG))],
        missing_parameter: MissingInput::Logged(message(MESSAGE, LOG)),
        stated: Ok(stated()),
    }
}

fn analyzed(mechanisms: Vec<MechanismInput>) -> ExpansionResult {
    analyze(&ExpansionInput {
        mechanisms,
        message_functions: BTreeSet::from([LOG]),
        allocations: BTreeSet::from([NEW]),
    })
}

fn effect_use(rows: Vec<Instruction>) -> UseCall {
    UseCall {
        role: UseRole::Host(ExpansionHost::Commands(BlockFamily::Effect)),
        rows,
    }
}

fn reasons(result: &ExpansionResult) -> Vec<&'static str> {
    result.gaps.iter().map(|gap| gap.cause.reason).collect()
}

#[test]
fn a_placeholder_on_a_new_object_gives_its_reader_as_host() {
    let result = analyzed(vec![mechanism(vec![effect_use(constructs_placeholder())])]);
    let expansion = &result.expansions[0];

    assert_eq!(
        expansion.hosts,
        GrammarProperty::Known(vec![ExpansionHost::Commands(BlockFamily::Effect)])
    );
    assert_eq!(
        expansion.stage,
        GrammarProperty::Known(ExpansionStage::Compile)
    );
    assert_eq!(
        expansion.checks,
        GrammarProperty::Partial(vec![ExpansionCheck::UnknownName])
    );
    assert_eq!(
        expansion.missing_parameter,
        GrammarProperty::Known(Some(MissingParameter::Diagnostic))
    );
    assert!(result.gaps.is_empty(), "{:?}", result.gaps);
}

#[test]
fn a_constructor_on_another_object_is_not_a_use() {
    let result = analyzed(vec![mechanism(vec![
        effect_use(constructs_placeholder()),
        effect_use(constructs_on_stack()),
    ])]);

    assert_eq!(
        result.expansions[0].hosts,
        GrammarProperty::Partial(vec![ExpansionHost::Commands(BlockFamily::Effect)])
    );
    assert_eq!(reasons(&result), ["placeholder-object"]);
}

#[test]
fn excluded_callers_give_no_host_and_unjoined_ones_keep_the_hosts_partial() {
    let excluded = UseCall {
        role: UseRole::Excluded,
        rows: constructs_on_stack(),
    };
    let only_excluded = analyzed(vec![mechanism(vec![excluded])]);
    assert_eq!(
        only_excluded.expansions[0].hosts,
        GrammarProperty::Unresolved
    );
    assert_eq!(reasons(&only_excluded), ["no-host"]);

    let unjoined = UseCall {
        role: UseRole::Unjoined,
        rows: constructs_placeholder(),
    };
    let result = analyzed(vec![mechanism(vec![
        effect_use(constructs_placeholder()),
        unjoined,
    ])]);
    assert_eq!(
        result.expansions[0].hosts,
        GrammarProperty::Partial(vec![ExpansionHost::Commands(BlockFamily::Effect)])
    );
    assert_eq!(reasons(&result), ["unjoined-reader"]);
}

#[test]
fn a_check_needs_its_literal_as_the_argument_of_a_message_call() {
    for site in [
        message(OTHER_MESSAGE, LOG),
        message(MESSAGE, OTHER_CALL),
        overwritten_message(),
        MessageSite {
            rows: Vec::new(),
            literal: MESSAGE,
        },
    ] {
        let mut input = mechanism(vec![effect_use(constructs_placeholder())]);
        input.checks = vec![(ExpansionCheck::UnknownName, site)];
        let result = analyzed(vec![input]);

        assert_eq!(result.expansions[0].checks, GrammarProperty::Unresolved);
        assert_eq!(reasons(&result), ["message-argument"]);
    }
}

#[test]
fn a_stage_needs_an_expected_caller_and_no_unclassified_one_in_each_link() {
    for links in [
        vec![],
        vec![vec![LinkCaller::Expected, LinkCaller::Other]],
        vec![vec![LinkCaller::Expected], vec![LinkCaller::Allowed]],
        vec![vec![LinkCaller::Expected], vec![]],
    ] {
        let mut input = mechanism(vec![effect_use(constructs_placeholder())]);
        input.stage.links = links;
        let result = analyzed(vec![input]);

        assert_eq!(result.expansions[0].stage, GrammarProperty::Unresolved);
        assert_eq!(reasons(&result), ["stage-chain"]);
    }
}

#[test]
fn several_definition_sources_keep_their_order_unestablished() {
    let mut input = mechanism(vec![effect_use(constructs_placeholder())]);
    input.definitions = vec![
        DefinitionInput::SameFile(true),
        DefinitionInput::Directory(Directory::Named("common/scripted_variables".into())),
    ];
    let result = analyzed(vec![input]);
    assert_eq!(
        result.expansions[0].definitions,
        GrammarProperty::Partial(vec![
            ExpansionDefinitions::SameFile,
            ExpansionDefinitions::Directory {
                directory: "common/scripted_variables".into()
            },
        ])
    );
    assert_eq!(reasons(&result), ["lookup-order"]);

    let mut input = mechanism(vec![effect_use(constructs_placeholder())]);
    input.definitions = vec![DefinitionInput::Directory(Directory::Ambiguous(vec![
        "common/a".into(),
        "common/b".into(),
    ]))];
    let result = analyzed(vec![input]);
    assert_eq!(
        result.expansions[0].definitions,
        GrammarProperty::Unresolved
    );
    assert_eq!(reasons(&result), ["definition-directory"]);
}

#[test]
fn unbound_stated_forms_leave_the_forms_and_a_stated_missing_parameter_unresolved() {
    let mut input = mechanism(vec![effect_use(constructs_placeholder())]);
    input.stated = Err("stated-form-functions");
    input.missing_parameter = MissingInput::Stated(MissingParameter::KeptAsText);
    let result = analyzed(vec![input]);
    let expansion = &result.expansions[0];

    assert_eq!(expansion.call_forms, GrammarProperty::Unresolved);
    assert_eq!(expansion.parameter_forms, GrammarProperty::Unresolved);
    assert_eq!(expansion.missing_parameter, GrammarProperty::Unresolved);
    assert_eq!(reasons(&result), ["stated-form-functions"]);
}
