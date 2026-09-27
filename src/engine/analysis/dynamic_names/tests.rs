//! The dynamic-name method on authored ARM64. The code is authored, not game code; the layouts
//! follow the M45 flag commands.
use super::*;
use crate::engine::analysis::assembler::{Arm64, arm64};
use crate::engine::analysis::declarations::{Composition, Function, ParserSlots, ScopeSlots};
use crate::engine::analysis::evaluate::ReadOnlyData;
use crate::engine::analysis::families::{StringFunctions, StringLayout};

const NEW: u64 = 0x9000;
const NAME_READER: u64 = 0x9100;
const INTERNER: u64 = 0x9200;
const SETTER: u64 = 0x9300;
const REMOVER: u64 = 0x9400;
const RESOLVER: u64 = 0x9500;
const UNKNOWN: u64 = 0x9600;
const STUB: u64 = 0x9700;

const ASSIGN: u64 = 0x2000;
const MEMBER: u64 = 0x2400;
const EXECUTE: u64 = 0x3000;
const EVALUATE: u64 = 0x3400;
const ACCESSOR: u64 = 0x3800;
const SCOPE_FLAGS: u64 = 0x3c00;
const OTHER_ACCESSOR: u64 = 0x4000;
const SCOPE_ACCESS: u64 = 0x4400;

const GLOBAL_STATE: u64 = 0x60_0450;
const GLOBAL_GUARD: u64 = 0x60_0458;

const INDEX: u32 = 0xa8;
const NAME: u32 = 0xb0;
const TARGET: u32 = 0xd8;
const ACCESSOR_SLOT: u64 = 0xe8;
const SLOTS: CommandSlots = CommandSlots {
    assign: 0x20,
    role: 0x50,
};

const COUNTRY: usize = 2;
const PLANET: usize = 3;

const SCOPE_ROUTE: Route = Route::Scope {
    terminal: SCOPE_FLAGS,
    offset: 0,
};

fn scope(bit: usize) -> ScopeType {
    let name = match bit {
        COUNTRY => "country",
        PLANET => "planet",
        _ => "other",
    };
    ScopeType {
        bit,
        name: name.into(),
    }
}

fn function(code: Arm64) -> Function {
    Function {
        address: code.start(),
        code: code.bytes(),
    }
}

/// One authored command: its name, declared scopes, and the function in each vtable slot.
struct Authored {
    name: &'static str,
    scopes: ScopeOutcome,
    slots: Vec<(u64, u64)>,
}

fn command(name: &'static str, scopes: &[usize], slots: &[(u64, u64)]) -> Authored {
    Authored {
        name,
        scopes: ScopeOutcome::Listed(scopes.iter().map(|&bit| scope(bit)).collect()),
        slots: slots.to_vec(),
    }
}

/// The slots of a flag command whose assign reader is `ASSIGN`, role slot `role` and accessor
/// `accessor`.
fn flag_slots(role: u64, accessor: u64) -> [(u64, u64); 3] {
    [
        (SLOTS.assign, ASSIGN),
        (SLOTS.role, role),
        (ACCESSOR_SLOT, accessor),
    ]
}

/// A family of `commands` whose slot functions are `bodies`. Each command gets a factory, a
/// create method and a vtable of its own; unset reader slots hold a stub.
fn family(kind: DeclarationKind, commands: Vec<Authored>, bodies: Vec<Arm64>) -> CommandFamily {
    let mut functions: BTreeMap<u64, Function> = bodies
        .into_iter()
        .map(function)
        .map(|body| (body.address, body))
        .collect();
    let mut stub = Arm64::at(STUB);
    arm64!(stub; ret);
    functions.insert(STUB, function(stub));
    let mut pointers = BTreeMap::new();
    let mut sites = Vec::new();
    for (index, authored) in commands.into_iter().enumerate() {
        let index = index as u64;
        let factory = 0x50_0010 + index * 0x100;
        let vtable = 0x40_0010 + index * 0x1000;
        let create = 0x1_0000 + index * 0x100;
        let mut code = Arm64::at(create);
        code.prologue();
        arm64!(code; mov w0, #16);
        code.call(NEW);
        arm64!(code; mov x19, x0);
        code.address(8, vtable);
        arm64!(code; str x8, [x19]; mov x0, x19);
        code.epilogue();
        arm64!(code; ret);
        functions.insert(create, function(code));
        pointers.insert(factory + 0x10, create);
        pointers.insert(vtable + 0x10, STUB);
        pointers.insert(vtable + 0x18, STUB);
        pointers.extend(
            authored
                .slots
                .iter()
                .map(|&(slot, target)| (vtable + slot, target)),
        );
        sites.push((
            create,
            Site::Declared {
                name: authored.name.into(),
                description: String::new(),
                usage: String::new(),
                scopes: authored.scopes,
                factory,
            },
        ));
    }

    CommandFamily {
        kind,
        declarations: DeclarationInput {
            tokens: BTreeMap::new(),
            registrars: vec![],
            register_entry: BTreeSet::new(),
            entry_helpers: BTreeSet::new(),
            operator_new: BTreeSet::from([NEW]),
            constructors: BTreeMap::new(),
            functions,
            pointers,
            pointer_data: std::sync::OnceLock::new(),
            strings: BTreeMap::new(),
            slots: ScopeSlots {
                create: 0x10,
                supported_scopes: 0x80,
            },
            parser_slots: ParserSlots {
                read: 0x10,
                member: 0x18,
                initializer: 0x90,
            },
            scope_names: Some(vec![
                "none".into(),
                "any".into(),
                "country".into(),
                "planet".into(),
            ]),
            composition: Composition {
                bodies: BTreeMap::new(),
                callers: BTreeMap::new(),
                composers: BTreeSet::new(),
                dynamic_token: BTreeSet::new(),
                create_database: BTreeSet::new(),
                strings: StringFunctions::default(),
                layout: StringLayout { flag_byte: 0x17 },
                data: ReadOnlyData::default(),
            },
        },
        inventory: DeclarationResult {
            sites,
            table_gaps: vec![],
        },
        slots: SLOTS,
    }
}

fn analyzed(families: Vec<CommandFamily>) -> BTreeMap<String, NameOutcome> {
    let input = DynamicNameInput {
        families,
        functions: FlagFunctions {
            name_reader: NAME_READER,
            interner: INTERNER,
            setter: SETTER,
            remover: REMOVER,
        },
        scope_type_offset: 0x8,
        names: BTreeMap::new(),
    };

    analyze(&input)
        .into_iter()
        .map(|command| (command.name, command.outcome))
        .collect()
}

/// The assign reader of a flag command: `name@target` is split into the name at `NAME` and the
/// target at `TARGET`; a static name is interned and its index stored at `index`.
fn assign(index: u32) -> Arm64 {
    let mut code = Arm64::at(ASSIGN);
    code.prologue();
    arm64!(code;
        mov x19, x0;
        add x1, x19, #NAME;
        add x2, x19, #TARGET
    );
    code.call(NAME_READER);
    arm64!(code; tbnz w0, #0, ->dynamic);
    code.call(INTERNER);
    arm64!(code; strh w0, [x19, #index]; ->dynamic:);
    code.epilogue();
    arm64!(code; ret);
    code
}

/// A member reader that splits and interns a name only for its `flag` key.
fn timed_member(index: u32) -> Arm64 {
    let mut code = Arm64::at(MEMBER);
    code.prologue();
    arm64!(code;
        mov x19, x0;
        cmp w2, #7; // token 7, the flag key
        b.ne ->other;
        add x1, x19, #0x2b8;
        add x2, x19, #0x2e0
    );
    code.call(NAME_READER);
    arm64!(code; tbnz w0, #0, ->done);
    code.call(INTERNER);
    arm64!(code; strh w0, [x19, #index]; b ->done; ->other:);
    code.call(UNKNOWN);
    arm64!(code; ->done:);
    code.epilogue();
    arm64!(code; ret);
    code
}

/// Where an execute slot finds the flags that it passes to its role.
#[derive(Clone, Copy)]
enum Store {
    Accessor,
    Owner,
}

/// The execute slot of a setter or remover: the resolver's flag for a dynamic name, else the
/// index stored at `index`, then `role` on the flags that the accessor slot returns.
fn execute(at: u64, index: u32, role: u64, store: Store) -> Arm64 {
    let mut code = Arm64::at(at);
    arm64!(code;
        mov x20, x0;
        mov x19, x1;
        ldrb w8, [x20, #0xc7]; // the dynamic name's short-string length
        cbz w8, ->static_name;
        mov x0, x19;
        add x1, x20, #TARGET;
        add x2, x20, #NAME
    );
    code.call(RESOLVER);
    arm64!(code;
        mov x21, x0;
        b ->access;
        ->static_name:;
        ldrh w21, [x20, #index];
        ->access:;
        ldr x8, [x20];
        ldr x8, [x8, #0xe8];
        mov x0, x20;
        mov x1, x19;
        blr x8
    );
    if let Store::Owner = store {
        arm64!(code; add x0, x20, #0x100); // flags inside the command
    }
    arm64!(code; and x1, x21, #0xffff);
    code.tail_call(role);
    code
}

/// The evaluate slot of a flag reader in the M45 membership-scan form, testing the index at
/// `index`.
fn evaluate(index: u32) -> Arm64 {
    let mut code = Arm64::at(EVALUATE);
    arm64!(code;
        stp x22, x21, [sp, #-0x30]!;
        stp x20, x19, [sp, #0x10];
        stp x29, x30, [sp, #0x20];
        add x29, sp, #0x20;
        mov x19, x1;
        mov x20, x0;
        ldrsb w8, [x0, #0xc7];
        tbnz w8, #31, ->long_name;
        and x8, x8, #0xff;
        cbz x8, ->static_name;
        ->dynamic:;
        add x2, x20, #NAME;
        add x1, x20, #TARGET;
        add x3, x20, #0x28;
        mov x0, x19;
        mov w4, #1
    );
    code.call(RESOLVER);
    arm64!(code;
        mov x21, x0;
        b ->access;
        ->long_name:;
        ldr x8, [x20, #0xb8];
        cbnz x8, ->dynamic;
        ->static_name:;
        ldrh w21, [x20, #index];
        ->access:;
        ldr x8, [x20];
        ldr x8, [x8, #0xe8];
        mov x0, x20;
        mov x1, x19;
        blr x8;
        ldr w8, [x0, #0x1c];
        cmp w8, #1;
        b.lt ->missing;
        mov x9, #0;
        ldr x10, [x0, #0x10];
        ->next:;
        ldrh w11, [x10, x9, lsl #1];
        cmp w11, w21, uxth;
        b.eq ->found;
        add x9, x9, #1;
        cmp x8, x9;
        b.ne ->next;
        ->missing:;
        mov w0, #0;
        ldp x29, x30, [sp, #0x20];
        ldp x20, x19, [sp, #0x10];
        ldp x22, x21, [sp], #0x30;
        ret;
        ->found:;
        cmn w9, #1;
        cset w0, ne;
        ldp x29, x30, [sp, #0x20];
        ldp x20, x19, [sp, #0x10];
        ldp x22, x21, [sp], #0x30;
        ret
    );
    code
}

/// An accessor that forwards the scope object to `terminal`.
fn forwarding(at: u64, terminal: u64) -> Arm64 {
    let mut code = Arm64::at(at);
    arm64!(code; mov x0, x1);
    code.tail_call(terminal);
    code
}

/// A scope's own flag accessor, whose store the method does not follow.
fn scope_flags() -> Arm64 {
    let mut code = Arm64::at(SCOPE_FLAGS);
    arm64!(code; ldr x0, [x0, #0x40]; ret);
    code
}

/// The global accessor: the game state's flags at 0x478, and at `logged` on the path that logs
/// when the guard byte is clear.
fn global_accessor(at: u64, logged: u32) -> Arm64 {
    let mut code = Arm64::at(at);
    code.prologue();
    code.address(8, GLOBAL_GUARD);
    arm64!(code; ldrb w8, [x8]; cbz w8, ->log);
    code.load(8, GLOBAL_STATE);
    arm64!(code; add x0, x8, #0x478);
    code.epilogue();
    arm64!(code; ret; ->log:);
    code.call(UNKNOWN);
    code.load(8, GLOBAL_STATE);
    arm64!(code; add x0, x8, #logged);
    code.epilogue();
    arm64!(code; ret);
    code
}

fn flag(outcome: &NameOutcome) -> &FlagCommand {
    match outcome {
        NameOutcome::Flag(flag) => flag,
        other => panic!("not a flag command: {other:?}"),
    }
}

fn routes(flag: &FlagCommand) -> Vec<(Role, usize, Result<Route, Unresolved>)> {
    flag.uses
        .iter()
        .map(|role_use| (role_use.role, role_use.scope.bit, role_use.route.clone()))
        .collect()
}

#[test]
fn a_setter_reader_and_remover_reach_the_scope_store_through_their_accessors() {
    let effects = family(
        DeclarationKind::Effect,
        vec![
            command(
                "set_flag",
                &[COUNTRY, PLANET],
                &flag_slots(EXECUTE, ACCESSOR),
            ),
            command(
                "remove_flag",
                &[COUNTRY],
                &flag_slots(EXECUTE + 0x100, ACCESSOR),
            ),
        ],
        vec![
            assign(INDEX),
            execute(EXECUTE, INDEX, SETTER, Store::Accessor),
            execute(EXECUTE + 0x100, INDEX, REMOVER, Store::Accessor),
            forwarding(ACCESSOR, SCOPE_FLAGS),
            scope_flags(),
        ],
    );
    // The reader's accessor reaches the terminal through the scope's own forwarding accessor.
    let mut scope_access = Arm64::at(SCOPE_ACCESS);
    scope_access.tail_call(SCOPE_FLAGS);
    let triggers = family(
        DeclarationKind::Trigger,
        vec![command(
            "has_flag",
            &[COUNTRY],
            &flag_slots(EVALUATE, OTHER_ACCESSOR),
        )],
        vec![
            assign(INDEX),
            evaluate(INDEX),
            forwarding(OTHER_ACCESSOR, SCOPE_ACCESS),
            scope_access,
            scope_flags(),
        ],
    );
    let outcomes = analyzed(vec![effects, triggers]);

    let setter = flag(&outcomes["set_flag"]);
    assert_eq!(setter.form, DynamicNameForm::TargetSuffix);
    assert_eq!(
        routes(setter),
        [
            (Role::Defines, COUNTRY, Ok(SCOPE_ROUTE)),
            (Role::Defines, PLANET, Ok(SCOPE_ROUTE))
        ]
    );
    assert_eq!(setter.stops, []);
    let remover = flag(&outcomes["remove_flag"]);
    assert_eq!(routes(remover), [(Role::Removes, COUNTRY, Ok(SCOPE_ROUTE))]);
    let reader = flag(&outcomes["has_flag"]);
    assert_eq!(reader.form, DynamicNameForm::TargetSuffix);
    assert_eq!(routes(reader), [(Role::Reads, COUNTRY, Ok(SCOPE_ROUTE))]);
}

#[test]
fn a_timed_setter_stores_its_name_in_the_member_reader() {
    const TIMED_INDEX: u32 = 0x2b4;
    let mut slots = flag_slots(EXECUTE, ACCESSOR).to_vec();
    slots.extend([(0x18, MEMBER), (SLOTS.assign, STUB)]);
    let effects = family(
        DeclarationKind::Effect,
        vec![command("set_timed_flag", &[COUNTRY], &slots)],
        vec![
            timed_member(TIMED_INDEX),
            execute(EXECUTE, TIMED_INDEX, SETTER, Store::Accessor),
            forwarding(ACCESSOR, SCOPE_FLAGS),
            scope_flags(),
        ],
    );
    let outcomes = analyzed(vec![effects]);

    let timed = flag(&outcomes["set_timed_flag"]);
    assert_eq!(routes(timed), [(Role::Defines, COUNTRY, Ok(SCOPE_ROUTE))]);
    // The resolver receives the name and target that the assign reader keeps, not the ones that
    // this member reader keeps, so the dynamic form is not joined.
    assert_eq!(timed.form, DynamicNameForm::Unresolved);
    assert!(timed.stops.contains(&Unresolved::new("dynamic-form")));
}

#[test]
fn a_global_accessor_gives_one_store_for_every_calling_scope() {
    let effects = family(
        DeclarationKind::Effect,
        vec![command(
            "set_global",
            &[COUNTRY],
            &flag_slots(EXECUTE, ACCESSOR),
        )],
        vec![
            assign(INDEX),
            execute(EXECUTE, INDEX, SETTER, Store::Accessor),
            global_accessor(ACCESSOR, 0x478),
        ],
    );
    let triggers = family(
        DeclarationKind::Trigger,
        vec![command(
            "has_global",
            &[COUNTRY, PLANET],
            &flag_slots(EVALUATE, OTHER_ACCESSOR),
        )],
        vec![
            assign(INDEX),
            evaluate(INDEX),
            global_accessor(OTHER_ACCESSOR, 0x478),
        ],
    );
    let outcomes = analyzed(vec![effects, triggers]);

    let global = Ok(Route::Global {
        address: GLOBAL_STATE,
        offset: 0x478,
    });
    assert_eq!(
        routes(flag(&outcomes["set_global"])),
        [(Role::Defines, COUNTRY, global.clone())]
    );
    assert_eq!(
        routes(flag(&outcomes["has_global"])),
        [
            (Role::Reads, COUNTRY, global.clone()),
            (Role::Reads, PLANET, global)
        ]
    );
}

#[test]
fn a_global_route_that_the_guarded_path_contradicts_forms_no_store() {
    let effects = family(
        DeclarationKind::Effect,
        vec![command(
            "set_global",
            &[COUNTRY],
            &flag_slots(EXECUTE, ACCESSOR),
        )],
        vec![
            assign(INDEX),
            execute(EXECUTE, INDEX, SETTER, Store::Accessor),
            global_accessor(ACCESSOR, 0x480),
        ],
    );
    let outcomes = analyzed(vec![effects]);

    assert_eq!(
        routes(flag(&outcomes["set_global"])),
        [(
            Role::Defines,
            COUNTRY,
            Err(Unresolved::new("accessor-routes"))
        )]
    );
}

#[test]
fn a_store_that_no_accessor_returned_gives_no_role() {
    let effects = family(
        DeclarationKind::Effect,
        vec![command(
            "set_owned",
            &[COUNTRY],
            &flag_slots(EXECUTE, ACCESSOR),
        )],
        vec![
            assign(INDEX),
            execute(EXECUTE, INDEX, SETTER, Store::Owner),
            forwarding(ACCESSOR, SCOPE_FLAGS),
            scope_flags(),
        ],
    );
    let outcomes = analyzed(vec![effects]);

    let owned = flag(&outcomes["set_owned"]);
    assert_eq!(routes(owned), []);
    assert!(owned.stops.contains(&Unresolved::new("role-store")));
    assert!(owned.stops.contains(&Unresolved::new("no-role")));
}

#[test]
fn a_setter_of_another_index_gives_no_role() {
    let effects = family(
        DeclarationKind::Effect,
        vec![command(
            "set_other",
            &[COUNTRY],
            &flag_slots(EXECUTE, ACCESSOR),
        )],
        vec![
            assign(INDEX),
            execute(EXECUTE, INDEX + 2, SETTER, Store::Accessor),
            forwarding(ACCESSOR, SCOPE_FLAGS),
            scope_flags(),
        ],
    );
    let outcomes = analyzed(vec![effects]);

    let other = flag(&outcomes["set_other"]);
    assert_eq!(routes(other), []);
    assert!(other.stops.contains(&Unresolved::new("role-flag")));
}

#[test]
fn a_scan_of_another_index_gives_no_read() {
    let triggers = family(
        DeclarationKind::Trigger,
        vec![command(
            "has_other",
            &[COUNTRY],
            &flag_slots(EVALUATE, ACCESSOR),
        )],
        vec![
            assign(INDEX),
            evaluate(INDEX + 2),
            forwarding(ACCESSOR, SCOPE_FLAGS),
            scope_flags(),
        ],
    );
    let outcomes = analyzed(vec![triggers]);

    let other = flag(&outcomes["has_other"]);
    assert_eq!(routes(other), []);
    assert!(other.stops.contains(&Unresolved::new("read-index")));
}

#[test]
fn a_name_interned_without_the_splitter_does_not_accept_a_target() {
    let mut plain = Arm64::at(ASSIGN);
    plain.prologue();
    arm64!(plain; mov x19, x0);
    plain.call(INTERNER);
    arm64!(plain; strh w0, [x19, #INDEX]);
    plain.epilogue();
    arm64!(plain; ret);
    let effects = family(
        DeclarationKind::Effect,
        vec![
            command("set_plain", &[COUNTRY], &flag_slots(EXECUTE, ACCESSOR)),
            command("unrelated", &[COUNTRY], &[(SLOTS.assign, STUB)]),
        ],
        vec![
            plain,
            execute(EXECUTE, INDEX, SETTER, Store::Accessor),
            forwarding(ACCESSOR, SCOPE_FLAGS),
            scope_flags(),
        ],
    );
    let outcomes = analyzed(vec![effects]);

    let plain = flag(&outcomes["set_plain"]);
    assert_eq!(plain.form, DynamicNameForm::NotAccepted);
    assert_eq!(routes(plain), [(Role::Defines, COUNTRY, Ok(SCOPE_ROUTE))]);
    assert_eq!(outcomes["unrelated"], NameOutcome::NotFlag);
}

#[test]
fn an_interned_name_stored_outside_the_command_is_unresolved() {
    let mut lost = Arm64::at(ASSIGN);
    lost.prologue();
    arm64!(lost; mov x19, x0);
    lost.call(INTERNER);
    arm64!(lost; strh w0, [x20, #INDEX]); // x20 is not the command
    lost.epilogue();
    arm64!(lost; ret);
    let effects = family(
        DeclarationKind::Effect,
        vec![command(
            "set_lost",
            &[COUNTRY],
            &flag_slots(EXECUTE, ACCESSOR),
        )],
        vec![lost],
    );
    let outcomes = analyzed(vec![effects]);

    assert_eq!(
        outcomes["set_lost"],
        NameOutcome::Unresolved(Unresolved::new("index-store"))
    );
}

#[test]
fn a_reader_path_that_does_not_return_leaves_the_command_unresolved() {
    let mut stalled = Arm64::at(ASSIGN);
    stalled.prologue();
    arm64!(stalled;
        mov x19, x0;
        add x1, x19, #NAME;
        add x2, x19, #TARGET
    );
    stalled.call(NAME_READER);
    arm64!(stalled; tbnz w0, #0, ->dynamic);
    stalled.call(INTERNER);
    arm64!(stalled; strh w0, [x19, #INDEX]);
    stalled.epilogue();
    arm64!(stalled;
        ret;
        ->dynamic:;
        br x9 // a branch to an unknown address
    );
    let effects = family(
        DeclarationKind::Effect,
        vec![command(
            "set_stalled",
            &[COUNTRY],
            &flag_slots(EXECUTE, ACCESSOR),
        )],
        vec![
            stalled,
            execute(EXECUTE, INDEX, SETTER, Store::Accessor),
            forwarding(ACCESSOR, SCOPE_FLAGS),
            scope_flags(),
        ],
    );
    let outcomes = analyzed(vec![effects]);

    let stalled = flag(&outcomes["set_stalled"]);
    assert!(
        stalled
            .stops
            .iter()
            .any(|stop| stop.reason == "branch-value")
    );
    assert_eq!(stalled.form, DynamicNameForm::Unresolved);
}

#[test]
fn a_name_that_only_an_unreadable_site_registers_is_not_examined() {
    let mut effects = family(
        DeclarationKind::Effect,
        vec![command(
            "set_flag",
            &[COUNTRY],
            &flag_slots(EXECUTE, ACCESSOR),
        )],
        vec![
            assign(INDEX),
            execute(EXECUTE, INDEX, SETTER, Store::Accessor),
            forwarding(ACCESSOR, SCOPE_FLAGS),
            scope_flags(),
        ],
    );
    effects.inventory.sites.push((
        0x7_0000,
        Site::Unreadable {
            name: Some("set_unreadable".into()),
            what: "entry-shape",
        },
    ));
    let outcomes = analyzed(vec![effects]);

    assert_eq!(
        outcomes["set_unreadable"],
        NameOutcome::NotExamined(Unresolved::new("command-registration"))
    );
    assert!(matches!(outcomes["set_flag"], NameOutcome::Flag(_)));
}
