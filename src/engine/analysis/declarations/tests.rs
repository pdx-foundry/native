//! The declaration method on small authored inputs. The code is authored ARM64, not game code.
use super::*;
use crate::engine::analysis::{
    assembler::{Arm64, arm64},
    evaluate::{ReadOnlyData, trace_causes},
    families::{StringFunctions, StringLayout},
    stop::CauseKind,
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
        constructors: BTreeMap::new(),
        functions: functions
            .into_iter()
            .map(|function| (function.address, function))
            .collect(),
        pointers: BTreeMap::new(),
        pointer_data: std::sync::OnceLock::new(),
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
        parser_slots: ParserSlots {
            read: 0x10,
            member: 0x18,
            initializer: 0x90,
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
        factory: FACTORY,
        description: description.into(),
        usage: usage.into(),
        scopes: ScopeOutcome::Unresolved(Unresolved::new("factory-create")),
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

#[test]
fn command_reader_retains_receiver_identity_and_requires_both_virtual_methods() {
    const READ: u64 = 0xb000;
    const MEMBER: u64 = 0xb100;
    let create = create(|body| {
        body.address(8, VTABLE);
        arm64!(body; str x8, [x19]);
    });
    let mut input = input(
        vec![win_registrar()],
        vec![create, constant_getter(READ, 0), constant_getter(MEMBER, 0)],
        composition(vec![], BTreeMap::new()),
    );
    input.pointers = BTreeMap::from([
        (FACTORY + 0x10, CREATE),
        (VTABLE + 0x80, SCOPE_GETTER),
        (VTABLE + 0x10, READ),
        (VTABLE + 0x18, MEMBER),
    ]);
    assert_eq!(
        command_reader(&input, FACTORY),
        Ok(CommandReader {
            vtable: VTABLE,
            read: READ,
            member: MEMBER
        })
    );
    input.pointers.remove(&(VTABLE + 0x18));
    assert_eq!(
        command_reader(&input, FACTORY),
        Err(Unresolved::new("command-member-slot"))
    );
    input.pointers.insert(VTABLE + 0x18, MEMBER);
    input.functions.remove(&MEMBER);
    assert_eq!(
        command_reader(&input, FACTORY),
        Err(Unresolved::new("command-reader-body"))
    );
    input.pointers.remove(&(FACTORY + 0x10));
    assert_eq!(
        command_reader(&input, FACTORY),
        Err(Unresolved::new("factory-create"))
    );
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
        factory: FACTORY,
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
        [win(ScopeOutcome::Unresolved(Unresolved::new("scope-mask")))]
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

#[test]
fn command_reader_requires_every_factory_path_to_return_the_same_live_vtable() {
    const OTHER: u64 = VTABLE + 0x1000;
    let mut body = Arm64::at(CREATE);
    body.prologue();
    arm64!(body; mov w0, #16);
    body.call(NEW);
    arm64!(body; mov x19, x0);
    body.address(8, VTABLE);
    arm64!(body; str x8, [x19]);
    let branch = body.here();
    arm64!(body; cbz x3, extern (branch + 16) as usize);
    body.address(8, OTHER);
    arm64!(body; str x8, [x19]; mov x0, x19);
    body.epilogue();
    arm64!(body; ret);
    let mut input = input(
        vec![],
        vec![function(body)],
        composition(vec![], BTreeMap::new()),
    );
    input.pointers.insert(FACTORY + 0x10, CREATE);
    assert_eq!(
        command_reader(&input, FACTORY),
        Err(Unresolved::new("ambiguous-command-vtable"))
    );

    let erased = create(|body| {
        body.address(8, VTABLE);
        arm64!(body; str x8, [x19]; str x4, [x19]);
    });
    input.functions.insert(CREATE, erased);
    assert_eq!(
        command_reader(&input, FACTORY),
        Err(Unresolved::new("command-vtable"))
    );

    let wrong = create(|body| {
        body.address(8, VTABLE);
        arm64!(body; str x8, [x20]);
    });
    input.functions.insert(CREATE, wrong);
    assert_eq!(
        command_reader(&input, FACTORY),
        Err(Unresolved::new("command-vtable"))
    );
}

#[test]
fn factory_joins_constructor_subobjects_without_conflating_their_readers() {
    const CONSTRUCTOR: u64 = 0xc000;
    const MEMBER_VTABLE: u64 = 0x35000;
    const READ: u64 = 0xb000;
    const MEMBER: u64 = 0xb100;
    let mut body = Arm64::at(CREATE);
    body.prologue();
    arm64!(body; mov w0, #128);
    body.call(NEW);
    arm64!(body; mov x19, x0);
    body.address(8, VTABLE);
    arm64!(body; str x8, [x19]; add x0, x19, #32);
    body.call(CONSTRUCTOR);
    arm64!(body; mov x0, x19);
    body.epilogue();
    arm64!(body; ret);
    let mut input = input(
        vec![],
        vec![
            function(body),
            constant_getter(READ, 0),
            constant_getter(MEMBER, 0),
        ],
        composition(vec![], BTreeMap::new()),
    );
    input.pointers = BTreeMap::from([
        (FACTORY + 0x10, CREATE),
        (VTABLE + 0x10, READ),
        (VTABLE + 0x18, MEMBER),
    ]);
    input.constructors.insert(
        CONSTRUCTOR,
        BTreeMap::from([(0, MEMBER_VTABLE), (8, MEMBER_VTABLE + 0x80)]),
    );
    assert_eq!(
        command_reader(&input, FACTORY),
        Ok(CommandReader {
            vtable: VTABLE,
            read: READ,
            member: MEMBER
        })
    );
    input.constructors.clear();
    assert_eq!(
        command_reader(&input, FACTORY),
        Err(Unresolved::new("command-vtable"))
    );
    input
        .constructors
        .insert(CONSTRUCTOR, BTreeMap::from([(128, MEMBER_VTABLE)]));
    assert_eq!(
        command_reader(&input, FACTORY).unwrap_err().reason,
        "constructor-vtable-bound"
    );
}

/// The receiver failure of the factory whose create method is `body`, traced. The untraced
/// failure must be the same obstruction, without a trace.
fn traced_receiver_failure(body: Arm64) -> (Unresolved, Vec<(CauseKind, u64)>, bool) {
    let mut input = input(
        vec![],
        vec![function(body)],
        composition(vec![], BTreeMap::new()),
    );
    input.pointers.insert(FACTORY + 0x10, CREATE);
    let untraced = command_reader(&input, FACTORY).unwrap_err();
    let traced = trace_causes(|| command_reader(&input, FACTORY)).unwrap_err();
    assert_eq!(traced, untraced);
    assert!(untraced.trace.is_none());

    let trace = traced.trace.as_deref().copied().unwrap();
    for cause in trace.causes() {
        assert_eq!(cause.entry, CREATE);
    }
    let causes = trace
        .causes()
        .map(|cause| (cause.kind, cause.instruction))
        .collect();
    (untraced, causes, trace.unrecorded)
}

/// A create method that allocates the command in `x19`, and returns where it called `NEW`.
fn allocating_create() -> (Arm64, u64) {
    let mut body = Arm64::at(CREATE);
    body.prologue();
    arm64!(body; mov w0, #16);
    let allocation = body.here();
    body.call(NEW);
    arm64!(body; mov x19, x0); // the command
    body.address(8, VTABLE);
    arm64!(body; str x8, [x19]);
    (body, allocation)
}

fn returning(mut body: Arm64) -> Arm64 {
    arm64!(body; mov x0, x19);
    body.epilogue();
    arm64!(body; ret);
    body
}

#[test]
fn an_unresolved_receiver_names_where_the_command_lost_its_vtable() {
    const CONSTRUCTOR: u64 = 0xc000;
    let (mut body, _) = allocating_create();
    arm64!(body; mov x0, x19);
    let unrecognized = body.here();
    body.call(CONSTRUCTOR); // an unrecognized call may change the command
    let lost = traced_receiver_failure(returning(body));
    let invalidated = vec![(CauseKind::Invalidated, unrecognized)];
    assert_eq!(
        lost,
        (Unresolved::new("command-vtable"), invalidated, false)
    );

    let (mut body, allocation) = allocating_create();
    arm64!(body; str x4, [x19]); // x4 is lost to the allocation call
    let lost = traced_receiver_failure(returning(body));
    let clobbered = vec![(CauseKind::Call, allocation)];
    assert_eq!(lost, (Unresolved::new("command-vtable"), clobbered, false));

    let (mut body, _) = allocating_create();
    let store = body.here();
    arm64!(body; str xzr, [x20]); // x20 is unknown, so this may overwrite the vtable
    let lost = traced_receiver_failure(returning(body));
    let overwritten = vec![(CauseKind::UnknownStore, store)];
    assert_eq!(
        lost,
        (Unresolved::new("command-vtable"), overwritten, false)
    );

    let mut body = Arm64::at(CREATE);
    body.prologue();
    arm64!(body; mov w0, #16);
    body.call(NEW);
    arm64!(body; mov x19, x0; str xzr, [x20]); // the command's vtable is never written
    let lost = traced_receiver_failure(returning(body));
    assert_eq!(lost, (Unresolved::new("command-vtable"), vec![], true));
}

#[test]
fn an_unknown_factory_return_names_the_call_that_gave_it() {
    const OTHER: u64 = 0xd000;
    let (mut body, _) = allocating_create();
    let returned = body.here();
    body.call(OTHER); // its return value is unknown
    body.epilogue();
    arm64!(body; ret);

    let lost = traced_receiver_failure(body);
    let call = vec![(CauseKind::Call, returned)];
    assert_eq!(lost, (Unresolved::new("factory-return"), call, false));
}

#[test]
fn a_create_method_that_tail_calls_an_out_of_line_factory_returns_its_command() {
    const FACTORY_BODY: u64 = 0xc000;
    const READ: u64 = 0xb000;
    const MEMBER: u64 = 0xb100;
    let mut forward = Arm64::at(CREATE);
    forward.tail_call(FACTORY_BODY);
    let out_of_line = returning(allocating_create_at(FACTORY_BODY));
    let mut input = input(
        vec![],
        vec![
            function(forward),
            constant_getter(READ, 0),
            constant_getter(MEMBER, 0),
        ],
        composition(vec![], BTreeMap::new()),
    );
    input.pointers = BTreeMap::from([
        (FACTORY + 0x10, CREATE),
        (VTABLE + 0x10, READ),
        (VTABLE + 0x18, MEMBER),
    ]);
    assert_eq!(
        command_reader(&input, FACTORY),
        Err(Unresolved::new("factory-return"))
    );

    input.functions.insert(FACTORY_BODY, function(out_of_line));
    assert_eq!(
        command_reader(&input, FACTORY),
        Ok(CommandReader {
            vtable: VTABLE,
            read: READ,
            member: MEMBER
        })
    );
}

/// A function at `start` that allocates the command in `x19` and installs its vtable.
fn allocating_create_at(start: u64) -> Arm64 {
    let mut body = Arm64::at(start);
    body.prologue();
    arm64!(body; mov w0, #16);
    body.call(NEW);
    arm64!(body; mov x19, x0); // the command
    body.address(8, VTABLE);
    arm64!(body; str x8, [x19]);
    body
}

#[test]
fn factory_member_constructor_preserves_only_the_primary_receiver() {
    for (offset, expected) in [
        (32, Ok(VTABLE)),
        (0, Err("command-vtable")),
        (128, Err("command-vtable")),
    ] {
        let input = member_constructor_factory(offset, false, false);
        assert_eq!(
            factory_vtable(&input, FACTORY).map_err(|stop| stop.reason),
            expected
        );
    }
}

#[test]
fn factory_enters_only_register_move_constructor_wrappers() {
    let input = member_constructor_factory(32, true, false);
    assert_eq!(factory_vtable(&input, FACTORY), Ok(VTABLE));
    let input = member_constructor_factory(32, true, true);
    assert_eq!(
        factory_vtable(&input, FACTORY).unwrap_err().reason,
        "command-vtable"
    );
}

fn member_constructor_factory(offset: u32, wrapper: bool, earlier_call: bool) -> DeclarationInput {
    const CONSTRUCTOR: u64 = 0xc000;
    const WRAPPER: u64 = 0xd000;
    let mut body = Arm64::at(CREATE);
    body.prologue();
    arm64!(body; mov w0, #128);
    body.call(NEW);
    arm64!(body; mov x19, x0);
    body.address(8, VTABLE);
    arm64!(body; str x8, [x19]; add x0, x19, #offset);
    if wrapper {
        arm64!(body; mov x8, x0; mov w0, #7);
        body.call(WRAPPER);
    } else {
        body.call(CONSTRUCTOR);
    }
    arm64!(body; mov x0, x19);
    body.epilogue();
    arm64!(body; ret);
    let mut functions = vec![function(body)];
    if wrapper {
        let mut body = Arm64::at(WRAPPER);
        if earlier_call {
            body.call(0xe000);
        }
        arm64!(body; mov x1, x0; mov x0, x8; b extern CONSTRUCTOR as usize);
        functions.push(function(body));
    }
    let mut input = input(vec![], functions, composition(vec![], BTreeMap::new()));
    input.pointers.insert(FACTORY + 0x10, CREATE);
    input.constructors.insert(CONSTRUCTOR, BTreeMap::new());
    input
}

#[test]
fn an_outside_member_constructor_keeps_the_unknown_call_fallback() {
    const CONSTRUCTOR: u64 = 0xc000;
    let mut body = Arm64::at(CREATE);
    body.prologue();
    arm64!(body; mov w0, #128);
    body.call(NEW);
    arm64!(body; mov x19, x0; mov x0, sp);
    body.call(CONSTRUCTOR);
    body.address(8, VTABLE);
    arm64!(body; str x8, [x19]; mov x0, x19);
    body.epilogue();
    arm64!(body; ret);
    let mut input = input(
        vec![],
        vec![function(body)],
        composition(vec![], BTreeMap::new()),
    );
    input.pointers.insert(FACTORY + 0x10, CREATE);
    input.constructors.insert(CONSTRUCTOR, BTreeMap::new());
    assert_eq!(factory_vtable(&input, FACTORY), Ok(VTABLE));
}

#[test]
fn factory_initial_state_follows_nested_constructors_and_call_arguments() {
    let mut input = initial_state_factory();
    let state = super::receiver::factory_state(&input, FACTORY).unwrap();
    assert_eq!(state.vtable, VTABLE);
    assert_eq!(
        crate::engine::analysis::durations::word(&state.bytes, 64),
        Some(7)
    );
    let point = (0..8).fold(0_u64, |point, index| {
        point | (u64::from(state.bytes[&(32 + index)]) << (index * 8))
    });
    assert_eq!(point, 0x35000);

    // Missing member code establishes neither an embedded point nor a literal.
    input.functions.remove(&0xd000);
    let state = super::receiver::factory_state(&input, FACTORY).unwrap();
    assert_eq!(
        crate::engine::analysis::durations::word(&state.bytes, 64),
        None
    );
    assert!(!state.bytes.contains_key(&32));
}

#[test]
fn factory_initial_state_rejects_changed_constructor_code_and_disagreed_literals() {
    let mut input = initial_state_factory();
    input.functions.get_mut(&0xd000).unwrap().code = arm64!(at 0xd000;
        brk #0
    );
    let state = super::receiver::factory_state(&input, FACTORY).unwrap();
    assert_eq!(state.vtable, VTABLE);
    assert!(!state.bytes.contains_key(&64));

    let mut input = initial_state_factory();
    let mut body = Arm64::at(0xd000);
    body.address(8, 0x35000);
    arm64!(body;
        str x8, [x0];
        cbz w2, extern 0xd01c;
        mov w1, #8;
        str w1, [x0, #32];
        ret;
        str w1, [x0, #32];
        ret
    );
    input.functions.insert(0xd000, function(body));
    let state = super::receiver::factory_state(&input, FACTORY).unwrap();
    assert!(!state.bytes.contains_key(&64));
    assert!(state.bytes.contains_key(&32));
}

fn initial_state_factory() -> DeclarationInput {
    const OWNER_CONSTRUCTOR: u64 = 0xc000;
    const OPERAND_CONSTRUCTOR: u64 = 0xd000;
    let mut create = Arm64::at(CREATE);
    create.prologue();
    arm64!(create; mov w0, #128);
    create.call(NEW);
    arm64!(create; mov x19, x0);
    create.call(OWNER_CONSTRUCTOR);
    arm64!(create; mov x0, x19);
    create.epilogue();
    arm64!(create; ret);

    let mut owner = Arm64::at(OWNER_CONSTRUCTOR);
    owner.prologue();
    arm64!(owner; mov x19, x0);
    owner.address(8, VTABLE);
    arm64!(owner; str x8, [x19]; add x0, x19, #32; mov w1, #7);
    owner.call(OPERAND_CONSTRUCTOR);
    arm64!(owner; mov x0, x19);
    owner.epilogue();
    arm64!(owner; ret);

    let mut operand = Arm64::at(OPERAND_CONSTRUCTOR);
    operand.address(8, 0x35000);
    arm64!(operand; str x8, [x0]; str w1, [x0, #32]; ret);
    let mut input = input(
        vec![],
        vec![function(create), function(owner), function(operand)],
        composition(vec![], BTreeMap::new()),
    );
    input.pointers.insert(FACTORY + 0x10, CREATE);
    input.constructors = BTreeMap::from([
        (OWNER_CONSTRUCTOR, BTreeMap::from([(0, VTABLE)])),
        (OPERAND_CONSTRUCTOR, BTreeMap::from([(0, 0x35000)])),
    ]);
    input
}

#[test]
fn factory_initial_state_withdraws_earlier_new_operands_after_an_unmodeled_call() {
    let mut input = initial_state_factory();
    let mut owner = Arm64::at(0xc000);
    owner.prologue();
    arm64!(owner; mov x19, x0);
    owner.address(8, VTABLE);
    arm64!(owner; str x8, [x19]; add x0, x19, #32; mov w1, #7);
    owner.call(0xd000);
    arm64!(owner; mov w8, #9; str w8, [x19, #80]; add x0, x19, #80);
    owner.call(0xe000);
    arm64!(owner; mov x0, x19);
    owner.epilogue();
    arm64!(owner; ret);
    input.functions.insert(0xc000, function(owner));
    let mut later = Arm64::at(0xe000);
    later.prologue();
    arm64!(later; mov x19, x0; mov w0, #1);
    later.call(0xf000); // unknown string lookup
    arm64!(later; mov x0, x19);
    later.epilogue();
    arm64!(later; ret);
    input.functions.insert(0xe000, function(later));
    input.constructors.insert(0xe000, BTreeMap::new());
    let state = super::receiver::factory_state(&input, FACTORY).unwrap();
    assert_eq!(
        crate::engine::analysis::durations::word(&state.bytes, 64),
        None
    );
    assert!(!state.bytes.contains_key(&80));

    input.constructors.remove(&0xd000);
    let state = super::receiver::factory_state(&input, FACTORY).unwrap();
    assert!(!state.bytes.contains_key(&32));
    assert!(!state.bytes.contains_key(&64));
}

#[test]
fn factory_tail_constructor_uses_the_same_initial_state_join_as_a_direct_call() {
    let mut input = initial_state_factory();
    let mut create = Arm64::at(CREATE);
    create.prologue();
    arm64!(create; mov w0, #128);
    create.call(NEW);
    create.epilogue();
    arm64!(create; b extern 0xc000);
    input.functions.insert(CREATE, function(create));
    let state = super::receiver::factory_state(&input, FACTORY).unwrap();
    assert_eq!(state.vtable, VTABLE);
    assert_eq!(
        crate::engine::analysis::durations::word(&state.bytes, 64),
        Some(7)
    );
}

#[test]
fn factory_initial_value_requires_constructor_vtable_agreement() {
    let mut input = initial_state_factory();
    let mut operand = Arm64::at(0xd000);
    operand.address(8, 0x35008); // conflicts with the bound constructor's address point
    arm64!(operand; str x8, [x0]; str w1, [x0, #32]; ret);
    input.functions.insert(0xd000, function(operand));
    let state = super::receiver::factory_state(&input, FACTORY).unwrap();
    assert_eq!(
        crate::engine::analysis::durations::word(&state.bytes, 64),
        None
    );
}

#[test]
fn nested_constructor_stores_cannot_preserve_earlier_owner_bytes() {
    for destination in ["earlier_member", "unknown", "earlier_member_as_stack"] {
        let mut input = initial_state_factory();
        let mut owner = Arm64::at(0xc000);
        owner.prologue();
        arm64!(owner; mov x19, x0);
        owner.address(8, VTABLE);
        arm64!(owner;
            str x8, [x19];
            mov w8, #7;
            str w8, [x19, #64];
            mov x1, x19;
            add x0, x19, #80;
            mov w2, #9
        );
        owner.call(0xd000);
        arm64!(owner; mov x0, x19);
        owner.epilogue();
        arm64!(owner; ret);
        input.functions.insert(0xc000, function(owner));
        let mut nested = Arm64::at(0xd000);
        nested.address(8, 0x35000);
        arm64!(nested; str x8, [x0]);
        match destination {
            "unknown" => arm64!(nested; str w2, [x3]), // may reach any owner byte
            "earlier_member_as_stack" => arm64!(nested; mov sp, x1; str w2, [x1, #64]),
            _ => arm64!(nested; str w2, [x1, #64]), // preceding member through another argument
        }
        arm64!(nested; ret);
        input.functions.insert(0xd000, function(nested));
        let state = super::receiver::factory_state(&input, FACTORY).unwrap();
        assert_eq!(state.vtable, VTABLE);
        assert!(!state.bytes.contains_key(&64), "destination: {destination}");
        assert!(!state.bytes.contains_key(&80));

        let mut wrapper = Arm64::at(0xd000);
        wrapper.prologue();
        wrapper.call(0xe000);
        arm64!(wrapper; brk #0); // loses the wrapper proof after an escaped child write
        let mut nested = Arm64::at(0xe000);
        match destination {
            "unknown" => arm64!(nested; str w2, [x3]),
            "earlier_member_as_stack" => arm64!(nested; mov sp, x1; str w2, [x1, #64]),
            _ => arm64!(nested; str w2, [x1, #64]),
        }
        arm64!(nested; ret);
        input.functions.insert(0xd000, function(wrapper));
        input.functions.insert(0xe000, function(nested));
        input.constructors.insert(0xe000, BTreeMap::new());
        let state = super::receiver::factory_state(&input, FACTORY).unwrap();
        assert!(!state.bytes.contains_key(&64));
    }
}

#[test]
fn unconfined_constructor_evidence_cannot_erase_baseline_or_keep_new_members() {
    for destination in [
        "earlier_member",
        "unknown",
        "preexisting_pointer",
        "stored_owner_pointer",
        "owner_call_return",
        "unsupported",
    ] {
        let mut input = initial_state_factory();
        let mut create = Arm64::at(CREATE);
        create.prologue();
        arm64!(create; mov w0, #128);
        create.call(NEW);
        arm64!(create; mov x19, x0);
        create.call(0xc000);
        arm64!(create;
            mov w8, #5;
            str w8, [x19, #96]; // factory evidence that exists without entering any constructor
            mov x1, x19;
            add x0, x19, #112;
            mov w2, #9
        );
        create.call(0xe000);
        arm64!(create; mov x0, x19);
        create.epilogue();
        arm64!(create; ret);
        input.functions.insert(CREATE, function(create));
        let mut later = Arm64::at(0xe000);
        match destination {
            "earlier_member" => arm64!(later; str w2, [x1, #64]),
            "unknown" => arm64!(later; str w2, [x3]),
            "preexisting_pointer" => {
                later.address(3, 0x80000);
                arm64!(later; str w2, [x3]);
            }
            "stored_owner_pointer" => {
                later.prologue();
                arm64!(later; str x1, [sp]; ldr x3, [sp]; str w2, [x3, #64]);
                later.epilogue();
            }
            "owner_call_return" => {
                later.prologue();
                arm64!(later; mov x0, x1);
                later.call(0xf000); // an unmodeled call receives this and may return an alias
                arm64!(later; mov w2, #9; str w2, [x0, #64]);
                later.epilogue();
                input.functions.insert(
                    0xf000,
                    Function {
                        address: 0xf000,
                        code: arm64!(at 0xf000; ret),
                    },
                );
            }
            _ => arm64!(later; brk #0),
        }
        arm64!(later; ret);
        input.functions.insert(0xe000, function(later));
        input
            .constructors
            .insert(0xe000, BTreeMap::from([(0, 0x36000)]));
        let state = super::receiver::factory_state(&input, FACTORY).unwrap();
        assert_eq!(state.vtable, VTABLE);
        assert_eq!(
            crate::engine::analysis::durations::word(&state.bytes, 96),
            Some(5)
        );
        assert!(!state.bytes.contains_key(&32), "destination: {destination}");
        assert!(!state.bytes.contains_key(&64), "destination: {destination}");
        let point = (0..8).fold(0_u64, |value, index| {
            value | (u64::from(state.bytes[&(112 + index)]) << (index * 8))
        });
        assert_eq!(point, 0x36000);
    }
}
