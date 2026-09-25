//! The declaration method on small authored inputs. The code is authored ARM64, not game code.
use super::*;
use crate::engine::analysis::{
    assembler::{Arm64, arm64},
    evaluate::ReadOnlyData,
    families::{StringFunctions, StringLayout},
};

const REGISTER: u64 = 0x9000;
const NEW: u64 = 0x9100;
const STRING_FROM_TEXT: u64 = 0x9200;
const DYNAMIC_TOKEN: u64 = 0x9300;
const HELPER: u64 = 0x4000;
const REGISTERING: u64 = 0x5000;
const COMPOSER: u64 = 0x6000;
const CALLER: u64 = 0x7000;
const UNKNOWN_CALLER: u64 = 0x8000;

const CREATE: u64 = 0xa000;
const SCOPE_GETTER: u64 = 0xa100;

const FACTORY: u64 = 0x30010;
const VTABLE: u64 = 0x31010;
const VTABLE_POINTER: u64 = 0x32008;
const WIN_DOCUMENTATION: u64 = 0x20000;
const HELPER_DOCUMENTATION: u64 = 0x20100;
const LIST_DOCUMENTATION: u64 = 0x20200;
const LIST_NAME: u64 = 0x20300;

fn function(code: Arm64) -> Function {
    Function {
        address: code.start(),
        code: code.bytes(),
    }
}

fn data() -> ReadOnlyData {
    let mut bytes = vec![0; 0x400];
    for (address, text) in [
        (WIN_DOCUMENTATION, "Wins the game\nwin = yes"),
        (HELPER_DOCUMENTATION, "Executes effects\nif = { <effects> }"),
        (
            LIST_DOCUMENTATION,
            "Each army\nevery_owned_army = { <effects> }",
        ),
        (LIST_NAME, "every_owned_army"),
    ] {
        let offset = (address - WIN_DOCUMENTATION) as usize;
        bytes[offset..offset + text.len()].copy_from_slice(text.as_bytes());
    }
    ReadOnlyData::new(vec![(WIN_DOCUMENTATION, bytes)])
}

fn input(
    registrars: Vec<Function>,
    functions: Vec<Function>,
    composition: Composition,
) -> DeclarationInput {
    DeclarationInput {
        tokens: BTreeMap::from([
            (7, "win".into()),
            (8, "if".into()),
            (9, "every_country".into()),
        ]),
        registrars,
        register_entry: BTreeSet::from([REGISTER]),
        entry_helpers: BTreeSet::from([HELPER]),
        operator_new: BTreeSet::from([NEW]),
        functions: functions
            .into_iter()
            .map(|function| (function.address, function))
            .collect(),
        pointers: BTreeMap::new(),
        strings: BTreeMap::from([
            (WIN_DOCUMENTATION, "Wins the game\nwin = yes".into()),
            (
                HELPER_DOCUMENTATION,
                "Executes effects\nif = { <effects> }".into(),
            ),
        ]),
        slots: ScopeSlots {
            create: 0x10,
            supported_scopes: 0x80,
        },
        scope_names: Some(vec!["none".into()]),
        composition,
    }
}

fn composition(bodies: Vec<Function>, callers: BTreeMap<u64, Vec<(u64, u64)>>) -> Composition {
    Composition {
        bodies: bodies
            .into_iter()
            .map(|function| (function.address, function))
            .collect(),
        callers,
        composers: BTreeSet::from([COMPOSER]),
        dynamic_token: BTreeSet::from([DYNAMIC_TOKEN]),
        create_database: BTreeSet::new(),
        strings: StringFunctions {
            from_text: BTreeSet::from([STRING_FROM_TEXT]),
            ..StringFunctions::default()
        },
        layout: StringLayout { flag_byte: 0x17 },
        data: data(),
    }
}

fn declared(name: &str, description: &str, usage: &str) -> Site {
    Site::Declared {
        name: name.into(),
        description: description.into(),
        usage: usage.into(),
        scopes: ScopeOutcome::Unresolved("factory-create"),
    }
}

fn sites(input: &DeclarationInput) -> Vec<Site> {
    analyze(input)
        .unwrap()
        .sites
        .into_iter()
        .map(|(_, site)| site)
        .collect()
}

#[test]
fn a_tail_call_to_the_register_function_is_a_registration() {
    let mut registrar = Arm64::at(0x1000);
    arm64!(registrar; mov w0, #16);
    registrar
        .call(NEW)
        .address(8, FACTORY)
        .address(9, WIN_DOCUMENTATION);
    arm64!(registrar;
        stp x8, x9, [x0]; // the entry
        mov x2, x0;
        mov w1, #7 // "win"
    );
    registrar.epilogue().tail_call(REGISTER);
    let registrar = function(registrar);

    let input = input(
        vec![registrar],
        vec![],
        composition(vec![], BTreeMap::new()),
    );
    assert_eq!(
        sites(&input),
        [declared("win", "Wins the game", "win = yes")]
    );
}

#[test]
fn a_registry_helper_call_has_the_documentation_argument_and_the_helpers_factory() {
    let mut helper = Arm64::at(HELPER);
    helper.prologue();
    arm64!(helper; mov x23, x2; mov w0, #16);
    helper.call(NEW).address(8, FACTORY);
    arm64!(helper; stp x8, x23, [x0]); // the entry
    helper.epilogue();
    arm64!(helper; ret);
    let helper = function(helper);

    let mut registrar = Arm64::at(0x1000);
    arm64!(registrar; mov w1, #8); // "if"
    registrar.address(2, HELPER_DOCUMENTATION).call(HELPER);
    let registrar = function(registrar);

    let input = input(
        vec![registrar],
        vec![helper],
        composition(vec![], BTreeMap::new()),
    );
    assert_eq!(
        sites(&input),
        [declared("if", "Executes effects", "if = { <effects> }")]
    );
}

#[test]
fn a_documentation_address_set_before_another_call_is_not_read() {
    let mut helper = Arm64::at(HELPER);
    helper.prologue();
    arm64!(helper; mov x23, x2; mov w0, #16);
    helper.call(NEW).address(8, FACTORY);
    arm64!(helper; stp x8, x23, [x0]); // the entry
    helper.epilogue();
    arm64!(helper; ret);
    let helper = function(helper);

    let mut registrar = Arm64::at(0x1000);
    registrar.address(2, HELPER_DOCUMENTATION).call(0x9400);
    arm64!(registrar; mov w1, #8); // "if"
    registrar.call(HELPER);
    let registrar = function(registrar);

    let input = input(
        vec![registrar],
        vec![helper],
        composition(vec![], BTreeMap::new()),
    );
    assert_eq!(
        sites(&input),
        [Site::Unreadable {
            name: Some("if".into()),
            what: "entry-shape"
        }]
    );
}

/// A function that registers the token in `w1` with the documentation text in `x2`, the shape
/// of a script list's registration helper.
fn registering() -> (Function, u64) {
    let mut code = Arm64::at(REGISTERING);
    code.prologue();
    arm64!(code; mov x19, x1; mov x20, x2; mov w0, #16);
    code.call(NEW).address(8, FACTORY);
    arm64!(code;
        stp x8, x20, [x0]; // the entry
        mov x2, x0;
        mov x1, x19
    );
    let site = code.here();
    code.call(REGISTER).epilogue();
    arm64!(code; ret);
    (function(code), site)
}

#[test]
fn a_run_time_name_is_followed_through_each_caller() {
    let (registering, site) = registering();
    let mut composer = Arm64::at(COMPOSER);
    composer.prologue();
    arm64!(composer; mov x1, x0; mov x0, x8);
    composer.call(STRING_FROM_TEXT).epilogue();
    arm64!(composer; ret);
    let composer = function(composer);

    let mut caller = Arm64::at(CALLER);
    caller.prologue();
    arm64!(caller;
        sub sp, sp, #0x40;
        add x8, sp, #0x10 // the composed string
    );
    caller.address(0, LIST_NAME).call(COMPOSER);
    arm64!(caller; add x0, sp, #0x10);
    caller.call(DYNAMIC_TOKEN);
    arm64!(caller; mov x1, x0); // the run-time token
    caller.address(2, LIST_DOCUMENTATION);
    let composed_call = caller.here();
    caller.call(REGISTERING);
    arm64!(caller; mov w1, #9); // "every_country"
    caller.address(2, WIN_DOCUMENTATION);
    let literal_call = caller.here();
    caller.call(REGISTERING);
    arm64!(caller; add sp, sp, #0x40);
    caller.epilogue();
    arm64!(caller; ret);
    let caller = function(caller);

    let mut unknown = Arm64::at(UNKNOWN_CALLER);
    unknown.prologue();
    arm64!(unknown; ldr w1, [x0]); // a token from memory
    unknown.address(2, WIN_DOCUMENTATION);
    let unknown_call = unknown.here();
    unknown.call(REGISTERING).epilogue();
    arm64!(unknown; ret);
    let unknown = function(unknown);

    let registrar = Function {
        address: REGISTERING,
        code: registering.code[..(site + 4 - REGISTERING) as usize].to_vec(),
    };
    let callers = BTreeMap::from([(
        REGISTERING,
        vec![
            (composed_call, CALLER),
            (literal_call, CALLER),
            (unknown_call, UNKNOWN_CALLER),
        ],
    )]);
    let input = input(
        vec![registrar],
        vec![],
        composition(vec![registering, composer, caller, unknown], callers),
    );
    assert_eq!(
        sites(&input),
        [
            declared(
                "every_owned_army",
                "Each army",
                "every_owned_army = { <effects> }"
            ),
            declared("every_country", "Wins the game", "win = yes"),
            Site::RuntimeToken { obstacle: "token" },
        ]
    );
}

#[test]
fn a_run_time_name_without_callers_is_a_gap() {
    let (registering, site) = registering();
    let registrar = Function {
        address: REGISTERING,
        code: registering.code[..(site + 4 - REGISTERING) as usize].to_vec(),
    };
    let input = input(
        vec![registrar],
        vec![],
        composition(vec![registering], BTreeMap::new()),
    );
    assert_eq!(sites(&input), [Site::RuntimeToken { obstacle: "token" }]);
}

/// A literal registration of `win` whose entry holds `FACTORY`.
fn win_registrar() -> Function {
    let mut code = Arm64::at(0x1000);
    arm64!(code; mov w0, #16);
    code.call(NEW)
        .address(8, FACTORY)
        .address(9, WIN_DOCUMENTATION);
    arm64!(code;
        stp x8, x9, [x0]; // the entry
        mov x2, x0;
        mov w1, #7 // "win"
    );
    code.call(REGISTER);
    function(code)
}

/// A create method that allocates the command in `x19`, and in which `store` stores its vtable.
fn create(store: impl FnOnce(&mut Arm64)) -> Function {
    let mut code = Arm64::at(CREATE);
    code.prologue();
    arm64!(code; mov w0, #16);
    code.call(NEW);
    arm64!(code; mov x19, x0); // the command
    store(&mut code);
    arm64!(code; mov x0, x19);
    code.epilogue();
    arm64!(code; ret);
    function(code)
}

fn constant_getter(address: u64, mask: u32) -> Function {
    let mut code = Arm64::at(address);
    arm64!(code; mov w0, #mask; ret);
    function(code)
}

/// `win` with a create method, a command vtable at `VTABLE`, and its scope getter.
fn followed(create: Function, scope_getter: Function) -> Vec<Site> {
    let mut input = input(
        vec![win_registrar()],
        vec![create, scope_getter],
        composition(vec![], BTreeMap::new()),
    );
    input.pointers = BTreeMap::from([
        (FACTORY + 0x10, CREATE),
        (VTABLE + 0x80, SCOPE_GETTER),
        (VTABLE_POINTER, VTABLE - 0x10),
    ]);
    input.scope_names = Some(vec!["none".into(), "planet".into(), "country".into()]);
    sites(&input)
}

fn scope(bit: usize, name: &str) -> ScopeType {
    ScopeType {
        bit,
        name: name.into(),
    }
}

fn win(scopes: ScopeOutcome) -> Site {
    Site::Declared {
        name: "win".into(),
        description: "Wins the game".into(),
        usage: "win = yes".into(),
        scopes,
    }
}

#[test]
fn a_zero_scope_mask_is_any() {
    let create = create(|body| {
        body.address(8, VTABLE);
        arm64!(body; str x8, [x19]);
    });
    assert_eq!(
        followed(create, constant_getter(SCOPE_GETTER, 0)),
        [win(ScopeOutcome::Any)]
    );
}

#[test]
fn a_multi_bit_scope_mask_lists_each_scope_in_bit_order() {
    let create = create(|body| {
        body.address(8, VTABLE);
        arm64!(body; str x8, [x19]);
    });
    assert_eq!(
        followed(create, constant_getter(SCOPE_GETTER, 0b110)),
        [win(ScopeOutcome::Listed(vec![
            scope(1, "planet"),
            scope(2, "country")
        ]))]
    );
}

#[test]
fn a_scope_getter_that_reads_the_command_is_unresolved_on_its_declaration() {
    let create = create(|body| {
        body.address(8, VTABLE);
        arm64!(body; str x8, [x19]);
    });
    let mut getter = Arm64::at(SCOPE_GETTER);
    arm64!(getter; ldr x0, [x0, #8]; ret); // a member of the command
    let getter = function(getter);

    assert_eq!(
        followed(create, getter),
        [win(ScopeOutcome::Unresolved("scope-mask"))]
    );
}

#[test]
fn a_command_vtable_loaded_through_a_pointer_is_followed() {
    let create = create(|body| {
        body.load(8, VTABLE_POINTER);
        arm64!(body; add x8, x8, #0x10; str x8, [x19]);
    });
    assert_eq!(
        followed(create, constant_getter(SCOPE_GETTER, 0b10)),
        [win(ScopeOutcome::Listed(vec![scope(1, "planet")]))]
    );
}

#[test]
fn a_vtable_stored_through_a_copy_of_the_command_register_is_followed() {
    let create = create(|body| {
        body.address(9, VTABLE);
        arm64!(body; mov x8, x19; str x9, [x8], #0x68);
        body.address(10, 0x33000);
        arm64!(body;
            str x10, [x8]; // after the post-index, x8 is a member
            mov x8, x19;
            add x8, x8, #0x70;
            str x10, [x8] // after the add, x8 is a member
        );
    });
    assert_eq!(
        followed(create, constant_getter(SCOPE_GETTER, 0)),
        [win(ScopeOutcome::Any)]
    );
}

#[test]
fn documentation_split_preserves_usage_without_terminal_newline() {
    assert_eq!(
        split_documentation("desc\nl1\nl2\n"),
        ("desc".into(), "l1\nl2".into())
    );
    assert_eq!(split_documentation("desc"), ("desc".into(), "".into()));
    assert_eq!(split_documentation("desc\n"), ("desc".into(), "".into()));
}

#[test]
fn constant_getter_ends_at_return() {
    let row = |operation: &str, operands: &str| Instruction {
        address: 0,
        bytes: [0; 4],
        operation: operation.into(),
        operands: operands.into(),
    };
    assert_eq!(
        constant_return(&[row("mov", "x0,#0"), row("ret", "")]),
        Some(0)
    );
    assert_eq!(
        constant_return(&[row("ldr", "x0,[x1]"), row("ret", "")]),
        None
    );
}
