//! The derived-name method on authored ARM64. The code is authored, not game code; its shapes
//! follow the tradition getters and post-read checks.
use super::*;
use crate::engine::analysis::assembler::{Arm64, arm64};
use crate::engine::analysis::evaluate::ReadOnlyData;
use crate::engine::analysis::families::DatabaseLayout;
use crate::engine::analysis::fields::{CollectionField, ReaderJoin, RootField};

const FROM_TEXT: u64 = 0x900;
const APPEND_STRING: u64 = 0x904;
const APPEND_TEXT: u64 = 0x908;
const FORMAT: u64 = 0x90c;
const ALLOCATE: u64 = 0x910;
const LENGTH: u64 = 0x91c;
const COPY: u64 = 0x920;
const NEVER_RETURNS: u64 = 0x924;
const MOVE_ASSIGN: u64 = 0x928;

const LOOKUP: u64 = 0x9a0;
const CHECK: u64 = 0x9a4;
const SPRITE_LOOKUP: u64 = 0x9a8;
const LOG: u64 = 0x9b0;

const CONSTRUCTOR: u64 = 0x100;
/// `x0` is a string object; it returns its text in `x0` and its length in `x1`.
const VIEW: u64 = 0x200;
const HELPER: u64 = 0x300;
const ROOT: u64 = 0x1000;
const ROOT_SPACING: u64 = 0x800;

const EMPTY: u64 = 0x5000;
const DESC: u64 = 0x5010;
const DELAYED: u64 = 0x5020;
const MISSING: u64 = 0x5030;
const PREFIX: u64 = 0x5040;
const FORMATTED: u64 = 0x5050;
const NOTICE: u64 = 0x5060;

/// The item's layout in these runs: the key object, a string field, a Boolean field, a byte that
/// no field names, a pointer to an unnamed string, and a collection of element pointers.
const KEY: u32 = 0x10;
const NAME: u32 = 0x40;
const FLAG: u32 = 0x80;
const STATE: u32 = 0x88;
const UNNAMED: u32 = 0x90;
const SWAPS: u32 = 0xa0;
const SWAP_COUNT: u32 = SWAPS + 0x14;

fn strings() -> StringFunctions {
    StringFunctions {
        from_text: [FROM_TEXT].into(),
        append_string: [APPEND_STRING].into(),
        append_text: [APPEND_TEXT].into(),
        formatters: [(FORMAT, 16)].into(),
        allocators: [ALLOCATE].into(),
        lengths: [LENGTH].into(),
        copies: [COPY].into(),
        never_return: [NEVER_RETURNS].into(),
        move_assigns: [MOVE_ASSIGN].into(),
        ..StringFunctions::default()
    }
}

fn data() -> ReadOnlyData {
    let mut bytes = vec![0; 0x80];
    for (address, text) in [
        (DESC, "_desc"),
        (DELAYED, "_delayed"),
        (MISSING, "Missing %s"),
        (PREFIX, "prefix_"),
        (FORMATTED, "%s_name"),
        (NOTICE, "Notice"),
    ] {
        let start = (address - EMPTY) as usize;
        bytes[start..start + text.len()].copy_from_slice(text.as_bytes());
    }
    ReadOnlyData::new(vec![(EMPTY, bytes)])
}

fn input() -> NameInput {
    let view = NameArgument::View { text: 0, length: 1 };
    let sink = |target, role, argument, unchecked_miss| Sink {
        target,
        role,
        argument,
        unchecked_miss,
        result: None,
    };
    NameInput {
        sinks: [
            (
                LOOKUP,
                sink(
                    Target::Localization,
                    Role::Lookup,
                    view,
                    Some(Miss::ShowsKey),
                ),
            ),
            (CHECK, sink(Target::Localization, Role::Check, view, None)),
            (
                SPRITE_LOOKUP,
                sink(Target::Sprite, Role::Lookup, NameArgument::Object(1), None),
            ),
        ]
        .into(),
        logs: [(LOG, LogArguments::Formatted)].into(),
    }
}

/// Copies the key object in `x2` into the item at `KEY`.
fn constructor() -> Arm64 {
    let mut code = Arm64::at(CONSTRUCTOR);
    arm64!(code;
        ldr x8, [x2];
        str x8, [x0, #KEY];
        ldr x8, [x2, #8];
        str x8, [x0, #KEY + 8];
        ldr x8, [x2, #16];
        str x8, [x0, #KEY + 16];
        ret
    );
    code
}

fn view() -> Arm64 {
    let mut code = Arm64::at(VIEW);
    arm64!(code;
        ldrsb w9, [x0, #0x17];
        tbnz w9, #31, ->long;
        and x1, x9, #0xff;
        ret;
        ->long:;
        ldr x1, [x0, #8];
        ldr x0, [x0];
        ret
    );
    code
}

/// A root that keeps the item in `x19` and has stack strings at `sp + 0x20` and `sp + 0x60`.
fn root(index: u64) -> Arm64 {
    let mut code = Arm64::at(ROOT + index * ROOT_SPACING);
    arm64!(code; mov x19, x0; sub sp, sp, #0x100);
    code
}

/// The stack string at `sp + offset` becomes the empty text followed by the string object at
/// `x19 + object`, then by the literal at `suffix`.
fn compose(code: &mut Arm64, offset: u32, object: u32, suffix: Option<u64>) {
    arm64!(code; add x0, sp, #offset);
    code.address(1, EMPTY).call(FROM_TEXT);
    arm64!(code; add x0, sp, #offset; add x1, x19, #object);
    code.call(APPEND_STRING);
    if let Some(suffix) = suffix {
        arm64!(code; add x0, sp, #offset);
        code.address(1, suffix).call(APPEND_TEXT);
    }
}

/// Call `sink` with the view of the stack string at `sp + offset`.
fn use_stack(code: &mut Arm64, offset: u32, sink: u64) {
    arm64!(code; add x0, sp, #offset);
    code.call(VIEW).call(sink);
}

/// Call `sink` with the view of the string object at `x19 + object`.
fn use_object(code: &mut Arm64, object: u32, sink: u64) {
    arm64!(code; add x0, x19, #object);
    code.call(VIEW).call(sink);
}

fn root_name(index: usize) -> String {
    format!("CItem::Root{index}() const")
}

fn family_input() -> FamilyInput {
    FamilyInput {
        registration: 0,
        category_offset: 0,
        database: DatabaseLayout {
            items_offset: 0x48,
            count_offset: 0x54,
        },
        definitions: None,
        strings: strings(),
        layout: StringLayout { flag_byte: 0x17 },
        data: data(),
    }
}

/// Run the method over `roots`, each used each time the engine uses an item, with `helpers`
/// entered.
fn analyze_roots(roots: Vec<Arm64>, helpers: Vec<Arm64>, storage: &Storage) -> NameResult {
    let entered: BTreeSet<u64> = [VIEW]
        .into_iter()
        .chain(helpers.iter().map(Arm64::start))
        .collect();
    let starts: Vec<u64> = roots.iter().map(Arm64::start).collect();
    let functions: Vec<(u64, Vec<u8>)> = [constructor(), view()]
        .into_iter()
        .chain(roots)
        .chain(helpers)
        .map(|code| (code.start(), code.bytes()))
        .collect();
    let ranges: Vec<(u64, &[u8])> = functions
        .iter()
        .map(|(start, bytes)| (*start, bytes.as_slice()))
        .collect();
    let registry = RegistryInput {
        roots: starts
            .into_iter()
            .enumerate()
            .map(|(index, function)| Root {
                function,
                name: root_name(index),
                stage: Stage::WhenUsed,
                sites: BTreeSet::new(),
            })
            .collect(),
        entered,
        constructors: vec![CONSTRUCTOR],
        loading: None,
        code: Code::decode(&ranges).expect("authored code decodes"),
    };
    analyze(&family_input(), &input(), &registry, storage)
}

fn analyze_root(code: Arm64, storage: &Storage) -> NameResult {
    analyze_roots(vec![code], Vec::new(), storage)
}

fn literal(text: &str) -> Part {
    Part::Literal(text.into())
}

fn field(path: &[&str]) -> Part {
    Part::Field(path.iter().map(|part| (*part).into()).collect())
}

fn key_desc() -> Vec<Part> {
    vec![Part::ItemKey, literal("_desc")]
}

fn key_delayed() -> Vec<Part> {
    vec![Part::ItemKey, literal("_delayed")]
}

/// The method's name with these parts.
fn named<'a>(result: &'a NameResult, parts: &[Part]) -> &'a Name {
    result
        .names
        .iter()
        .find(|name| name.parts == parts)
        .unwrap_or_else(|| panic!("no name {parts:?} in {:#?}", result.names))
}

fn string_field(path: &[&str], offset: u32) -> StoredField {
    StoredField {
        path: path.iter().map(|part| (*part).into()).collect(),
        offset: offset.into(),
        element: None,
    }
}

/// A storage whose only string field is `name` at `NAME`.
fn name_storage() -> Storage {
    Storage {
        strings: vec![string_field(&["name"], NAME)],
        ..Storage::default()
    }
}

fn unresolved_field_condition(path: &[&str]) -> Condition {
    Condition::All(vec![
        Term::Unresolved,
        Term::FieldZero {
            path: path.iter().map(|part| (*part).into()).collect(),
            zero: false,
        },
    ])
}

#[test]
fn a_literal_and_the_key_looked_up_without_a_check_show_the_key_and_are_always_used() {
    let mut code = root(0);
    arm64!(code; add x0, sp, #0x20);
    code.address(1, PREFIX).call(FROM_TEXT);
    arm64!(code; add x0, sp, #0x20; add x1, x19, #KEY);
    code.call(APPEND_STRING);
    use_stack(&mut code, 0x20, LOOKUP);
    arm64!(code; ret);

    let result = analyze_root(code, &Storage::default());

    assert_eq!(result.key_offset, Ok(KEY.into()));
    assert_eq!(
        result.names,
        [Name {
            parts: vec![literal("prefix_"), Part::ItemKey],
            target: Target::Localization,
            stage: Stage::WhenUsed,
            on_missing: Miss::ShowsKey,
            condition: Condition::Always,
        }]
    );
    assert!(result.failures.is_empty(), "{:?}", result.failures);
}

#[test]
fn an_unchecked_lookup_without_a_stated_rule_has_an_unresolved_miss() {
    let mut code = root(0);
    compose(&mut code, 0x20, KEY, Some(DESC));
    arm64!(code; add x1, sp, #0x20);
    code.call(SPRITE_LOOKUP);
    arm64!(code; ret);

    let result = analyze_root(code, &Storage::default());

    let name = named(&result, &key_desc());
    assert_eq!(name.target, Target::Sprite);
    assert_eq!(name.on_missing, Miss::Unresolved);
    assert_eq!(name.condition, Condition::Always);
}

#[test]
fn a_check_whose_miss_logs_the_name_is_a_diagnostic() {
    let mut code = root(0);
    compose(&mut code, 0x20, KEY, Some(DESC));
    use_stack(&mut code, 0x20, CHECK);
    arm64!(code; tbnz w0, #0, ->found; add x0, sp, #0x20);
    code.call(VIEW);
    arm64!(code; str x0, [sp]);
    code.address(1, MISSING).call(LOG);
    arm64!(code; ->found:; ret);

    let result = analyze_root(code, &Storage::default());

    let name = named(&result, &key_desc());
    assert_eq!(name.on_missing, Miss::Diagnostic);
    assert_eq!(name.condition, Condition::Always);
}

#[test]
fn an_unrelated_or_unconditional_log_is_no_diagnostic() {
    let mut unrelated = root(0);
    compose(&mut unrelated, 0x20, KEY, Some(DESC));
    use_stack(&mut unrelated, 0x20, CHECK);
    arm64!(unrelated; tbnz w0, #0, ->found);
    unrelated.address(1, NOTICE).call(LOG);
    arm64!(unrelated; ->found:; ret);

    let mut unconditional = root(1);
    compose(&mut unconditional, 0x20, KEY, Some(DELAYED));
    use_stack(&mut unconditional, 0x20, CHECK);
    arm64!(unconditional; add x0, sp, #0x20);
    unconditional.call(VIEW);
    arm64!(unconditional; str x0, [sp]);
    unconditional.address(1, MISSING).call(LOG);
    arm64!(unconditional; ret);

    let result = analyze_roots(
        vec![unrelated, unconditional],
        Vec::new(),
        &Storage::default(),
    );

    assert_eq!(named(&result, &key_desc()).on_missing, Miss::Silent);
    assert_eq!(named(&result, &key_delayed()).on_missing, Miss::Unresolved);
}

#[test]
fn a_check_whose_miss_skips_the_name_is_silent() {
    let mut code = root(0);
    compose(&mut code, 0x20, KEY, Some(DESC));
    use_stack(&mut code, 0x20, CHECK);
    arm64!(code; tbz w0, #0, ->missing);
    use_stack(&mut code, 0x20, LOOKUP);
    arm64!(code; ->missing:; ret);

    let result = analyze_root(code, &Storage::default());

    let name = named(&result, &key_desc());
    assert_eq!(name.on_missing, Miss::Silent);
    assert_eq!(name.condition, Condition::Always);
}

/// The `GetDesc` shape: the field's name with a suffix is checked; when it is missing, the key
/// with the suffix replaces it in one string, which one check and one lookup then use. A path
/// that skips the field (`STATE` zero) uses the key's name at once.
#[test]
fn a_missing_name_that_another_name_replaces_at_the_same_sites_falls_back() {
    let mut code = root(0);
    arm64!(code; ldrb w8, [x19, #STATE]; cbz w8, ->base);
    compose(&mut code, 0x20, NAME, Some(DESC));
    use_stack(&mut code, 0x20, CHECK);
    arm64!(code; tbnz w0, #0, ->use_name; ->base:);
    compose(&mut code, 0x20, KEY, Some(DESC));
    arm64!(code; ->use_name:);
    use_stack(&mut code, 0x20, CHECK);
    arm64!(code; tbz w0, #0, ->done);
    use_stack(&mut code, 0x20, LOOKUP);
    arm64!(code; ->done:; ret);

    let result = analyze_root(code, &name_storage());

    let swap = named(&result, &[field(&["name"]), literal("_desc")]);
    assert_eq!(swap.on_missing, Miss::Fallback(key_desc()));
    assert_eq!(swap.condition, unresolved_field_condition(&["name"]));
    let base = named(&result, &key_desc());
    assert_eq!(base.on_missing, Miss::Silent);
    assert_eq!(base.condition, Condition::Unresolved);
}

#[test]
fn a_lookup_of_another_name_after_a_check_is_no_fallback() {
    let mut code = root(0);
    compose(&mut code, 0x20, KEY, Some(DESC));
    use_stack(&mut code, 0x20, CHECK);
    compose(&mut code, 0x60, KEY, Some(DELAYED));
    use_stack(&mut code, 0x60, LOOKUP);
    arm64!(code; ret);

    let result = analyze_root(code, &Storage::default());

    assert_eq!(named(&result, &key_desc()).on_missing, Miss::Silent);
    assert_eq!(named(&result, &key_delayed()).on_missing, Miss::ShowsKey);
}

#[test]
fn a_lookup_that_a_miss_triggers_at_another_site_is_no_fallback() {
    let mut code = root(0);
    compose(&mut code, 0x20, KEY, Some(DESC));
    use_stack(&mut code, 0x20, CHECK);
    arm64!(code; tbz w0, #0, ->missing);
    use_stack(&mut code, 0x20, LOOKUP);
    arm64!(code; b ->done; ->missing:);
    compose(&mut code, 0x60, KEY, Some(DELAYED));
    use_stack(&mut code, 0x60, LOOKUP);
    arm64!(code; ->done:; ret);

    let result = analyze_root(code, &Storage::default());

    let name = named(&result, &key_desc());
    assert!(!matches!(name.on_missing, Miss::Fallback(_)), "{name:?}");
}

#[test]
fn one_name_checked_at_two_sites_has_one_outcome() {
    let mut code = root(0);
    compose(&mut code, 0x20, KEY, Some(DESC));
    use_stack(&mut code, 0x20, CHECK);
    arm64!(code; tbz w0, #0, ->done);
    use_stack(&mut code, 0x20, CHECK);
    arm64!(code; tbnz w0, #0, ->done);
    compose(&mut code, 0x60, KEY, Some(DELAYED));
    use_stack(&mut code, 0x60, LOOKUP);
    arm64!(code; ->done:; ret);

    let result = analyze_root(code, &Storage::default());

    assert!(
        !result.names.iter().any(|name| name.parts == key_delayed()),
        "{:#?}",
        result.names
    );
}

#[test]
fn two_names_checked_at_one_helper_site_have_independent_outcomes() {
    let mut helper = Arm64::at(HELPER);
    arm64!(helper; stp x29, x30, [sp, #-16]!);
    helper.call(VIEW).call(CHECK);
    arm64!(helper; ldp x29, x30, [sp], #16; ret);

    let mut code = root(0);
    compose(&mut code, 0x20, KEY, Some(DESC));
    arm64!(code; add x0, sp, #0x20);
    code.call(HELPER);
    arm64!(code; tbz w0, #0, ->desc_missing);
    use_stack(&mut code, 0x20, LOOKUP);
    arm64!(code; ->desc_missing:);
    compose(&mut code, 0x60, KEY, Some(DELAYED));
    arm64!(code; add x0, sp, #0x60);
    code.call(HELPER);
    arm64!(code; tbz w0, #0, ->delayed_missing);
    use_stack(&mut code, 0x60, LOOKUP);
    arm64!(code; ->delayed_missing:; ret);

    let result = analyze_roots(vec![code], vec![helper], &Storage::default());

    assert_eq!(named(&result, &key_desc()).on_missing, Miss::Silent);
    assert_eq!(named(&result, &key_delayed()).on_missing, Miss::Silent);
}

/// The field's text is copied inline into a stack string, the suffix appended, and the string
/// moved into another one, whose view the lookup receives.
#[test]
fn a_field_copied_inline_appended_and_moved_names_its_field() {
    let mut code = root(0);
    arm64!(code;
        add x8, x19, #NAME;
        ldrsb w9, [x8, #0x17];
        ldr x10, [x8];
        cmp w9, #0;
        csel x20, x10, x8, lt;
        mov x0, x20
    );
    code.call(LENGTH);
    arm64!(code; mov x21, x0; add x1, x21, #1);
    code.call(ALLOCATE);
    arm64!(code;
        mov x22, x0;
        str x22, [sp, #0x20];
        str x21, [sp, #0x28];
        mov x8, #0x8000000000000000;
        add x8, x8, #0x40;
        str x8, [sp, #0x30];
        mov x0, x22;
        mov x1, x20;
        mov x2, x21
    );
    code.call(COPY);
    arm64!(code; strb wzr, [x22, x21]; add x0, sp, #0x20);
    code.address(1, DESC).call(APPEND_TEXT);
    arm64!(code; add x0, sp, #0x60; add x1, sp, #0x20);
    code.call(MOVE_ASSIGN);
    use_stack(&mut code, 0x60, LOOKUP);
    arm64!(code; ret);

    let result = analyze_root(code, &name_storage());

    let name = named(&result, &[field(&["name"]), literal("_desc")]);
    assert_eq!(name.on_missing, Miss::ShowsKey);
    assert_eq!(name.condition, unresolved_field_condition(&["name"]));
}

#[test]
fn a_field_name_is_never_always_used_and_needs_its_field_nonempty() {
    let mut code = root(0);
    use_object(&mut code, NAME, LOOKUP);
    compose(&mut code, 0x20, NAME, Some(DESC));
    use_stack(&mut code, 0x20, LOOKUP);
    arm64!(code; ret);

    let result = analyze_root(code, &name_storage());

    for parts in [
        vec![field(&["name"])],
        vec![field(&["name"]), literal("_desc")],
    ] {
        let name = named(&result, &parts);
        assert_eq!(name.condition, unresolved_field_condition(&["name"]));
    }
}

#[test]
fn two_names_that_content_selects_for_one_site_are_both_found_and_neither_always() {
    let mut code = root(0);
    arm64!(code; ldrb w8, [x19, #STATE]; cbz w8, ->delayed);
    compose(&mut code, 0x20, KEY, Some(DESC));
    arm64!(code; b ->look_up; ->delayed:);
    compose(&mut code, 0x20, KEY, Some(DELAYED));
    arm64!(code; ->look_up:);
    use_stack(&mut code, 0x20, LOOKUP);
    arm64!(code; ret);

    let result = analyze_root(code, &Storage::default());

    for parts in [key_desc(), key_delayed()] {
        assert_eq!(named(&result, &parts).condition, Condition::Unresolved);
    }
}

#[test]
fn a_name_that_needs_a_collection_element_keeps_the_empty_collection_path() {
    let mut code = root(0);
    arm64!(code; ldr w8, [x19, #SWAP_COUNT]; cbz w8, ->empty);
    compose(&mut code, 0x20, KEY, Some(DESC));
    use_stack(&mut code, 0x20, LOOKUP);
    arm64!(code; ->empty:; ret);
    let storage = Storage {
        collections: vec![Collection {
            offset: SWAPS.into(),
            data_offset: 8,
            count_offset: 0x14,
        }],
        ..name_storage()
    };

    let result = analyze_root(code, &storage);

    assert_eq!(named(&result, &key_desc()).condition, Condition::Unresolved);
}

#[test]
fn a_site_with_an_unresolved_name_keeps_its_gap_when_a_template_run_names_one_alternative() {
    let mut code = root(0);
    arm64!(code; ldrb w8, [x19, #STATE]; cbz w8, ->unnamed; add x0, x19, #NAME; b ->look_up);
    arm64!(code; ->unnamed:; ldr x0, [x19, #UNNAMED]; ->look_up:);
    code.call(VIEW).call(LOOKUP);
    arm64!(code; ret);

    let result = analyze_root(code, &name_storage());

    named(&result, &[field(&["name"])]);
    assert_eq!(result.failures.get("unresolved-name"), Some(&1));
}

/// An unnamed string is copied, so its text is assumed empty, and its length then chooses
/// between two names. The path for an empty text is taken; the other name is never found.
#[test]
fn a_length_test_after_assumed_text_leaves_the_unexplored_name_a_gap() {
    let mut code = root(0);
    arm64!(code; ldr x8, [x19, #UNNAMED]; str x8, [sp, #0x10]);
    arm64!(code; add x0, sp, #0x20);
    code.address(1, EMPTY).call(FROM_TEXT);
    arm64!(code; add x0, sp, #0x20; ldr x1, [sp, #0x10]);
    code.call(APPEND_STRING);
    arm64!(code; add x0, sp, #0x20);
    code.call(VIEW);
    arm64!(code; cbnz x1, ->nonempty);
    compose(&mut code, 0x60, KEY, Some(DESC));
    arm64!(code; b ->look_up; ->nonempty:);
    compose(&mut code, 0x60, KEY, Some(DELAYED));
    arm64!(code; ->look_up:);
    use_stack(&mut code, 0x60, LOOKUP);
    arm64!(code; ret);

    let result = analyze_root(code, &Storage::default());

    assert!(result.failures.contains_key("assumed-text"));
    let found = named(&result, &key_desc());
    assert_eq!(found.condition, Condition::Unresolved);
    assert_eq!(found.on_missing, Miss::Unresolved);
    assert!(!result.names.iter().any(|name| name.parts == key_delayed()));
}

#[test]
fn a_name_that_a_tested_flag_selects_carries_the_flag_condition() {
    let mut code = root(0);
    arm64!(code; ldrb w8, [x19, #FLAG]; cbnz w8, ->skip);
    compose(&mut code, 0x20, KEY, Some(DESC));
    use_stack(&mut code, 0x20, LOOKUP);
    arm64!(code; ->skip:; ret);
    let storage = Storage {
        flags: vec![string_field(&["flag"], FLAG)],
        selections: vec![StorageSelection {
            method: root_name(0),
            field: vec!["name".into()],
            tested: vec!["flag".into()],
            zero: true,
        }],
        ..Storage::default()
    };

    let result = analyze_root(code, &storage);

    assert_eq!(
        named(&result, &key_desc()).condition,
        Condition::All(vec![
            Term::Unresolved,
            Term::FieldZero {
                path: vec!["flag".into()],
                zero: true,
            },
        ])
    );
}

#[test]
fn a_failed_path_beside_a_returned_one_keeps_a_name_from_always_and_leaves_a_gap() {
    let mut code = root(0);
    arm64!(code; ldrb w8, [x19, #STATE]; cbz w8, ->look_up; ldr x8, [x19, #UNNAMED]; br x8);
    arm64!(code; ->look_up:);
    compose(&mut code, 0x20, KEY, Some(DESC));
    use_stack(&mut code, 0x20, LOOKUP);
    arm64!(code; ret);

    let result = analyze_root(code, &Storage::default());

    assert_eq!(named(&result, &key_desc()).condition, Condition::Unresolved);
    assert!(
        result
            .failures
            .keys()
            .any(|reason| !["unresolved-name", "assumed-text"].contains(reason)),
        "{:?}",
        result.failures
    );
}

#[test]
fn a_name_that_a_fixed_buffer_bounds_is_a_gap() {
    let mut code = root(0);
    arm64!(code;
        ldrsb w9, [x19, #KEY + 0x17];
        ldr x10, [x19, #KEY];
        add x11, x19, #KEY;
        cmp w9, #0;
        csel x8, x10, x11, lt;
        str x8, [sp];
        add x0, sp, #0x40
    );
    code.address(1, FORMATTED).call(FORMAT);
    arm64!(code; mov x20, x0);
    code.call(LENGTH);
    arm64!(code; mov x1, x0; mov x0, x20);
    code.call(LOOKUP);
    arm64!(code; ret);

    let result = analyze_root(code, &Storage::default());

    assert!(result.names.is_empty(), "{:#?}", result.names);
    assert_eq!(result.failures.get("name-limit"), Some(&1));
}

/// A root field with the token `token` that `kind`'s reader stores at `offset`.
fn stored(name: &str, token: i64, kind: crate::ReaderKind, offset: i64) -> RootField {
    RootField {
        name: name.into(),
        token,
        constructor: 0,
        paths: Vec::new(),
        readers: vec![ReaderJoin::Stored {
            callee: "reader".into(),
            kind,
            destination: offset,
            repeat: crate::RepeatBehavior::Replace,
        }],
    }
}

fn field_result(
    fields: Vec<RootField>,
    collections: Vec<CollectionField>,
    uses: Vec<StorageSelection>,
) -> RegistryFieldResult {
    RegistryFieldResult {
        uses,
        persistent: BTreeMap::new(),
        persistent_points: BTreeMap::new(),
        scoped_destinations: BTreeMap::new(),
        stored_words: BTreeMap::new(),
        collections,
        fields,
        paths: Vec::new(),
        gaps: Vec::new(),
        partition_accounted: true,
    }
}

#[test]
fn storage_reads_string_and_boolean_fields_of_the_item_and_its_collections() {
    use crate::ReaderKind::{Block, Boolean, Integer, String};

    let selection = StorageSelection {
        method: root_name(0),
        field: vec!["swap".into(), "name".into()],
        tested: vec!["swap".into(), "inherit".into()],
        zero: true,
    };
    let swap = CollectionField {
        token: 3,
        offset: SWAPS.into(),
        data_offset: Some(8),
        count_offset: Some(0x14),
        class: "CSwap".into(),
        reader: None,
        fields: Box::new(field_result(
            vec![
                stored("name", 1, String, 0xf8),
                stored("inherit", 2, Boolean, 0x4f1),
            ],
            Vec::new(),
            Vec::new(),
        )),
    };
    let result = field_result(
        vec![
            stored("name", 1, String, NAME.into()),
            stored("cost", 2, Integer, 0x50),
            stored("swap", 3, Block, SWAPS.into()),
        ],
        vec![swap],
        vec![selection.clone()],
    );

    let storage = storage(&result);

    assert_eq!(
        storage.collections,
        [Collection {
            offset: SWAPS.into(),
            data_offset: 8,
            count_offset: 0x14,
        }]
    );
    assert_eq!(storage.selections, [selection]);
    assert_eq!(
        storage.strings,
        [
            string_field(&["name"], NAME),
            StoredField {
                path: vec!["swap".into(), "name".into()],
                offset: 0xf8,
                element: Some(0),
            },
        ]
    );
    assert_eq!(
        storage.flags,
        [StoredField {
            path: vec!["swap".into(), "inherit".into()],
            offset: 0x4f1,
            element: Some(0),
        }]
    );
}

#[test]
fn a_root_that_tests_too_many_flags_is_a_gap() {
    let mut code = root(0);
    compose(&mut code, 0x20, KEY, Some(DESC));
    use_stack(&mut code, 0x20, LOOKUP);
    arm64!(code; ret);
    let flags: Vec<StoredField> = (0..=FLAG_BOUND as u32)
        .map(|index| string_field(&[&format!("flag{index}")], FLAG + index))
        .collect();
    let storage = Storage {
        selections: flags
            .iter()
            .map(|flag| StorageSelection {
                method: root_name(0),
                field: vec!["name".into()],
                tested: flag.path.clone(),
                zero: true,
            })
            .collect(),
        flags,
        ..Storage::default()
    };

    let result = analyze_root(code, &storage);

    assert!(result.names.is_empty());
    assert_eq!(result.failures.get("flag-bound"), Some(&1));
}
