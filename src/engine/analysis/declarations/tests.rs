//! The declaration method on small authored inputs. The code is authored ARM64, not game code.
use super::*;
use crate::engine::analysis::{
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

/// Authored code at one start address.
struct Assembly {
    start: u64,
    words: Vec<u32>,
}

impl Assembly {
    fn at(start: u64) -> Self {
        Self {
            start,
            words: Vec::new(),
        }
    }

    fn here(&self) -> u64 {
        self.start + self.words.len() as u64 * 4
    }

    fn word(mut self, word: u32) -> Self {
        self.words.push(word);
        self
    }

    /// `stp x29, x30, [sp, #-16]!` and `mov x29, sp`.
    fn prologue(self) -> Self {
        self.word(0xa9bf7bfd).word(0x910003fd)
    }

    /// `ldp x29, x30, [sp], #16`.
    fn epilogue(self) -> Self {
        self.word(0xa8c17bfd)
    }

    fn ret(self) -> Self {
        self.word(0xd65f03c0)
    }

    fn mov_immediate(self, register: u32, value: u32) -> Self {
        self.word(0x52800000 | value << 5 | register)
    }

    fn mov(self, destination: u32, source: u32) -> Self {
        self.word(0xaa0003e0 | source << 16 | destination)
    }

    /// `adrp` and `add`: the address in `register`.
    fn address(self, register: u32, address: u64) -> Self {
        let pages = (address >> 12) as i64 - (self.here() >> 12) as i64;
        let pages = pages as u32;
        let adrp = 0x90000000 | (pages & 3) << 29 | (pages >> 2 & 0x7ffff) << 5 | register;
        let add = 0x91000000 | ((address & 0xfff) as u32) << 10 | register << 5 | register;
        self.word(adrp).word(add)
    }

    /// `add destination, sp, #offset`.
    fn stack_address(self, destination: u32, offset: u32) -> Self {
        self.word(0x91000000 | offset << 10 | 31 << 5 | destination)
    }

    /// `stp first, second, [x0]`.
    fn store_pair(self, first: u32, second: u32) -> Self {
        self.word(0xa9000000 | second << 10 | first)
    }

    /// `adrp` and `ldr`: the pointer stored at `address` in `register`.
    fn load(self, register: u32, address: u64) -> Self {
        let pages = (address >> 12) as i64 - (self.here() >> 12) as i64;
        let pages = pages as u32;
        let adrp = 0x90000000 | (pages & 3) << 29 | (pages >> 2 & 0x7ffff) << 5 | register;
        let ldr = 0xf9400000 | ((address & 0xfff) as u32 / 8) << 10 | register << 5 | register;
        self.word(adrp).word(ldr)
    }

    /// `add register, register, #value`.
    fn add(self, register: u32, value: u32) -> Self {
        self.word(0x91000000 | value << 10 | register << 5 | register)
    }

    /// `str source, [base]`.
    fn store(self, source: u32, base: u32) -> Self {
        self.word(0xf9000000 | base << 5 | source)
    }

    fn call(self, target: u64) -> Self {
        let offset = ((target as i64 - self.here() as i64) / 4) as u32 & 0x3ffffff;
        self.word(0x94000000 | offset)
    }

    fn tail_call(self, target: u64) -> Self {
        let offset = ((target as i64 - self.here() as i64) / 4) as u32 & 0x3ffffff;
        self.word(0x14000000 | offset)
    }

    fn function(self) -> Function {
        Function {
            address: self.start,
            code: self.words.into_iter().flat_map(u32::to_le_bytes).collect(),
        }
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
    let registrar = Assembly::at(0x1000)
        .mov_immediate(0, 16)
        .call(NEW)
        .address(8, FACTORY)
        .address(9, WIN_DOCUMENTATION)
        .store_pair(8, 9)
        .mov(2, 0)
        .mov_immediate(1, 7)
        .epilogue()
        .tail_call(REGISTER)
        .function();
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
    let helper = Assembly::at(HELPER)
        .prologue()
        .mov(23, 2)
        .mov_immediate(0, 16)
        .call(NEW)
        .address(8, FACTORY)
        .store_pair(8, 23)
        .epilogue()
        .ret()
        .function();
    let registrar = Assembly::at(0x1000)
        .mov_immediate(1, 8)
        .address(2, HELPER_DOCUMENTATION)
        .call(HELPER)
        .function();
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
    let helper = Assembly::at(HELPER)
        .prologue()
        .mov(23, 2)
        .mov_immediate(0, 16)
        .call(NEW)
        .address(8, FACTORY)
        .store_pair(8, 23)
        .epilogue()
        .ret()
        .function();
    let registrar = Assembly::at(0x1000)
        .address(2, HELPER_DOCUMENTATION)
        .call(0x9400)
        .mov_immediate(1, 8)
        .call(HELPER)
        .function();
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
    let before = Assembly::at(REGISTERING)
        .prologue()
        .mov(19, 1)
        .mov(20, 2)
        .mov_immediate(0, 16)
        .call(NEW)
        .address(8, FACTORY)
        .store_pair(8, 20)
        .mov(2, 0)
        .mov(1, 19);
    let site = before.here();
    (before.call(REGISTER).epilogue().ret().function(), site)
}

#[test]
fn a_run_time_name_is_followed_through_each_caller() {
    let (registering, site) = registering();
    let composer = Assembly::at(COMPOSER)
        .prologue()
        .mov(1, 0)
        .mov(0, 8)
        .call(STRING_FROM_TEXT)
        .epilogue()
        .ret()
        .function();
    let caller = Assembly::at(CALLER)
        .prologue()
        .word(0xd10103ff) // sub sp, sp, #0x40
        .stack_address(8, 0x10)
        .address(0, LIST_NAME)
        .call(COMPOSER)
        .stack_address(0, 0x10)
        .call(DYNAMIC_TOKEN)
        .mov(1, 0)
        .address(2, LIST_DOCUMENTATION);
    let composed_call = caller.here();
    let caller = caller
        .call(REGISTERING)
        .mov_immediate(1, 9)
        .address(2, WIN_DOCUMENTATION);
    let literal_call = caller.here();
    let caller = caller
        .call(REGISTERING)
        .word(0x910103ff) // add sp, sp, #0x40
        .epilogue()
        .ret()
        .function();
    let unknown = Assembly::at(UNKNOWN_CALLER)
        .prologue()
        .word(0xb9400001) // ldr w1, [x0]
        .address(2, WIN_DOCUMENTATION);
    let unknown_call = unknown.here();
    let unknown = unknown.call(REGISTERING).epilogue().ret().function();

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
    Assembly::at(0x1000)
        .mov_immediate(0, 16)
        .call(NEW)
        .address(8, FACTORY)
        .address(9, WIN_DOCUMENTATION)
        .store_pair(8, 9)
        .mov(2, 0)
        .mov_immediate(1, 7)
        .call(REGISTER)
        .function()
}

/// A create method that allocates the command in `x19`, and in which `store` stores its vtable.
fn create(store: impl FnOnce(Assembly) -> Assembly) -> Function {
    let body = Assembly::at(CREATE)
        .prologue()
        .mov_immediate(0, 16)
        .call(NEW)
        .mov(19, 0);
    store(body).mov(0, 19).epilogue().ret().function()
}

fn constant_getter(address: u64, mask: u32) -> Function {
    Assembly::at(address)
        .mov_immediate(0, mask)
        .ret()
        .function()
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
    let create = create(|body| body.address(8, VTABLE).store(8, 19));
    assert_eq!(
        followed(create, constant_getter(SCOPE_GETTER, 0)),
        [win(ScopeOutcome::Any)]
    );
}

#[test]
fn a_multi_bit_scope_mask_lists_each_scope_in_bit_order() {
    let create = create(|body| body.address(8, VTABLE).store(8, 19));
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
    let create = create(|body| body.address(8, VTABLE).store(8, 19));
    let getter = Assembly::at(SCOPE_GETTER)
        .word(0xf9400400) // ldr x0, [x0, #8]
        .ret()
        .function();
    assert_eq!(
        followed(create, getter),
        [win(ScopeOutcome::Unresolved("scope-mask"))]
    );
}

#[test]
fn a_command_vtable_loaded_through_a_pointer_is_followed() {
    let create = create(|body| body.load(8, VTABLE_POINTER).add(8, 0x10).store(8, 19));
    assert_eq!(
        followed(create, constant_getter(SCOPE_GETTER, 0b10)),
        [win(ScopeOutcome::Listed(vec![scope(1, "planet")]))]
    );
}

#[test]
fn a_vtable_stored_through_a_copy_of_the_command_register_is_followed() {
    let create = create(|body| {
        body.address(9, VTABLE)
            .mov(8, 19)
            .word(0xf8068509) // str x9, [x8], #0x68
            .address(10, 0x33000)
            .word(0xf900010a) // str x10, [x8]: after the post-index, x8 is a member
            .mov(8, 19)
            .add(8, 0x70)
            .word(0xf900010a) // str x10, [x8]: after the add, x8 is a member
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
