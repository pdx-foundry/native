//! Authored-input tests of the callback method, with negative controls.
use std::collections::{BTreeMap, BTreeSet};

use super::*;

// Rows call the engine functions by these addresses: the firing function 0x9000, the list
// firing 0x9010, the lookup 0x9020, the scripted rule 0x9030, and an unknown function 0x9900.
const LOOKUP: u64 = 0x9020;
const STRING: u64 = 0x9100;
const STRING_END: u64 = 0x9108;
const STRCMP: u64 = 0x9300;

const FRESH: u64 = 0x8000;
const SET_COUNTRY: u64 = 0x8100;
const SET_LEADER: u64 = 0x8200;
const CLEAR: u64 = 0x8300;
const COPY: u64 = 0x8400;
const PASSES_ON: u64 = 0x8500;

/// `on_test` is at 0x5000 and `on_other` at 0x5010.
const ON_TEST: u64 = 0x5000;
const INSTANCE: u64 = 0x6f00;

const COUNTRY: Slot = Slot::Scope(2);
const LEADER: Slot = Slot::Scope(8);

type Rows<'a> = &'a [(u64, &'a str, &'a str)];

fn rows(lines: Rows<'_>) -> Vec<Instruction> {
    lines
        .iter()
        .map(|(address, operation, operands)| Instruction {
            address: *address,
            bytes: [0; 4],
            operation: (*operation).into(),
            operands: (*operands).into(),
        })
        .collect()
}

/// A fresh constructor, two typed setters, the link reset and a copy, as the engine writes them.
fn scope_code() -> Vec<Instruction> {
    rows(&[
        (FRESH, "str", "xzr,[x0,#0x8]"),
        (FRESH + 4, "stp", "x0,x0,[x0,#0x30]"),
        (FRESH + 8, "str", "x0,[x0,#0x40]"),
        (FRESH + 12, "ret", ""),
        (SET_COUNTRY, "mov", "w9,#0x4"),
        (SET_COUNTRY + 4, "str", "x9,[x0,#0x8]"),
        (SET_COUNTRY + 8, "ret", ""),
        (SET_LEADER, "mov", "w9,#0x100"),
        (SET_LEADER + 4, "str", "x9,[x0,#0x8]"),
        (SET_LEADER + 8, "ret", ""),
        (CLEAR, "stp", "x0,x0,[x0,#0x30]"),
        (CLEAR + 4, "str", "x0,[x0,#0x40]"),
        (CLEAR + 8, "ret", ""),
        (PASSES_ON, "mov", "w9,#0x4"),
        (PASSES_ON + 4, "str", "x9,[x0,#0x8]"),
        (PASSES_ON + 8, "bl", "#0x9900"),
        (PASSES_ON + 12, "ret", ""),
    ])
}

fn data() -> ReadOnlyData {
    let mut strings = vec![0u8; 0x20];
    strings[..7].copy_from_slice(b"on_test");
    strings[0x10..0x18].copy_from_slice(b"on_other");
    ReadOnlyData::new(vec![(ON_TEST, strings)])
}

fn layout() -> CallbackLayout {
    CallbackLayout {
        scope_type_offset: 0x8,
        scope_root_offset: 0x30,
        scope_from_offset: 0x38,
        scope_prev_offset: 0x40,
        scripted_rules: RuleArray {
            base: 0,
            stride: 0xc0,
        },
        weighted_rules: RuleArray {
            base: 0x9cc0,
            stride: 0x40,
        },
        declaration_token_offset: 0,
    }
}

/// Rule declarations: scripted rules 0 and 1, and weighted rule 0.
fn rule_tables() -> RuleTables {
    RuleTables {
        code: rows(&[
            (0x6000, "adrp", "x8,#0x7000"),
            (0x6004, "mov", "w9,#0x11"),
            (0x6008, "str", "w9,[x8]"),
            (0x600c, "mov", "w9,#0x12"),
            (0x6010, "str", "w9,[x8,#0x20]"),
            (0x6014, "mov", "w9,#0x13"),
            (0x6018, "str", "w9,[x8,#0x400]"),
            (0x601c, "ret", ""),
            (0x6100, "cmp", "w0,#2"),
            (0x6104, "b.hs", "#0x6114"),
            (0x6108, "adrp", "x8,#0x7000"),
            (0x610c, "add", "x0,x8,x0,lsl#5"),
            (0x6110, "ret", ""),
            (0x6114, "adrp", "x0,#0x7800"),
            (0x6118, "ret", ""),
            (0x6200, "cbnz", "w0,#0x6210"),
            (0x6204, "adrp", "x0,#0x7000"),
            (0x6208, "add", "x0,x0,#0x400"),
            (0x620c, "ret", ""),
            (0x6210, "adrp", "x0,#0x7800"),
            (0x6214, "ret", ""),
        ]),
        initializer: 0x6000,
        finders: vec![
            (RuleFamily::Scripted, 0x6100),
            (RuleFamily::Weighted, 0x6200),
        ],
    }
}

struct Program {
    functions: BTreeMap<u64, Vec<Instruction>>,
    sites: Vec<Site>,
    forwarders: Vec<Forwarder>,
    rule_owners: BTreeSet<u64>,
    pulse: Option<Pulse>,
}

impl Program {
    fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
            sites: Vec::new(),
            forwarders: Vec::new(),
            rule_owners: BTreeSet::new(),
            pulse: None,
        }
    }

    fn function(mut self, lines: Rows<'_>) -> Self {
        self.functions.insert(lines[0].0, rows(lines));
        self
    }

    /// A site at `address` in the function that starts at `function`.
    fn site(mut self, function: u64, address: u64, call: SiteCall) -> Self {
        self.sites.push(Site {
            address,
            function,
            call,
        });
        self
    }

    fn forwarder(mut self, function: u64, kind: ForwarderKind) -> Self {
        self.forwarders.push(Forwarder { function, kind });
        self
    }

    fn input(self) -> CallbacksInput {
        CallbacksInput {
            functions: self.functions,
            scope_code: scope_code(),
            scope_functions: ScopeFunctions {
                fresh_constructors: BTreeSet::from([FRESH]),
                setters: BTreeSet::from([SET_COUNTRY, SET_LEADER, CLEAR, PASSES_ON]),
                copies: BTreeSet::from([COPY]),
                destructors: BTreeSet::new(),
                readers: BTreeSet::new(),
            },
            strings: StringFunctions {
                from_literal: BTreeSet::from([STRING]),
                copy: BTreeSet::new(),
                destructors: BTreeSet::from([STRING_END]),
                object_size: 0x18,
            },
            lookups: BTreeSet::from([LOOKUP]),
            sites: self.sites,
            forwarders: self.forwarders,
            rule_owners: self.rule_owners,
            script_fired_sites: 0,
            pulse: self.pulse,
            rule_tables: rule_tables(),
            tokens: BTreeMap::from([
                (0x11, "can_a".into()),
                (0x12, "can_b".into()),
                (0x13, "weight_c".into()),
            ]),
            scope_names: None,
            data: data(),
            layout: layout(),
        }
    }
}

const FIRE: SiteCall = SiteCall::Fire {
    name: 1,
    scope: SiteScope::Register(2),
};

fn context(this: Slot, root: Slot, from: &[Slot]) -> Context {
    Context {
        this,
        root,
        from: from.to_vec(),
    }
}

fn on_actions(input: &CallbacksInput) -> CallbacksResult {
    analyze(input, Family::OnAction).unwrap()
}

fn contexts(result: &CallbacksResult, name: &str) -> Vec<Context> {
    result.on_actions[name].contexts.iter().cloned().collect()
}

/// A function that builds scope A at sp+0x100 of the country type and the name `on_test`, then
/// runs `middle` and fires at 0x1100.
fn fires_country(middle: Rows<'_>) -> Vec<(u64, &str, &str)> {
    let mut lines = vec![
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x100"),
        (0x1008, "bl", "#0x8000"),
        (0x100c, "add", "x0,sp,#0x100"),
        (0x1010, "bl", "#0x8100"),
        (0x1014, "add", "x0,sp,#0x10"),
        (0x1018, "adrp", "x1,#0x5000"),
        (0x101c, "bl", "#0x9100"),
    ];
    lines.extend_from_slice(middle);
    let end = lines.last().map_or(0x1000, |(address, _, _)| *address);
    lines.extend(
        (end + 4..0x10f8)
            .step_by(4)
            .map(|address| (address, "nop", "")),
    );
    lines.extend([
        (0x10f8, "add", "x1,sp,#0x10"),
        (0x10fc, "add", "x2,sp,#0x100"),
        (0x1100, "bl", "#0x9000"),
        (0x1104, "ret", ""),
    ]);
    lines
}

fn fire_country(middle: Rows<'_>) -> CallbacksResult {
    let lines = fires_country(middle);
    on_actions(
        &Program::new()
            .function(&lines)
            .site(0x1000, 0x1100, FIRE)
            .input(),
    )
}

#[test]
fn a_literal_name_and_a_typed_scope_give_one_context() {
    let result = fire_country(&[]);

    assert_eq!(
        contexts(&result, "on_test"),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
    assert!(result.on_actions["on_test"].unresolved.is_empty());
    assert!(result.unnamed.is_empty());
}

#[test]
fn a_fresh_scope_has_no_type_and_links_to_itself() {
    let lines = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x100"),
        (0x1008, "bl", "#0x8000"),
        (0x100c, "add", "x0,sp,#0x10"),
        (0x1010, "adrp", "x1,#0x5000"),
        (0x1014, "bl", "#0x9100"),
        (0x1018, "add", "x1,sp,#0x10"),
        (0x101c, "add", "x2,sp,#0x100"),
        (0x1020, "bl", "#0x9000"),
        (0x1024, "ret", ""),
    ];
    let result = on_actions(
        &Program::new()
            .function(&lines)
            .site(0x1000, 0x1020, FIRE)
            .input(),
    );

    assert_eq!(
        contexts(&result, "on_test"),
        [context(Slot::NotSet, Slot::SelfLink, &[Slot::SelfLink])]
    );
}

#[test]
fn from_links_form_a_chain_that_ends_at_a_self_link() {
    let result = fire_country(&[
        (0x1020, "add", "x0,sp,#0x180"),
        (0x1024, "bl", "#0x8000"),
        (0x1028, "add", "x0,sp,#0x180"),
        (0x102c, "bl", "#0x8200"),
        (0x1030, "add", "x8,sp,#0x180"),
        (0x1034, "str", "x8,[sp,#0x138]"),
        (0x1038, "str", "x8,[sp,#0x130]"),
    ]);

    assert_eq!(
        contexts(&result, "on_test"),
        [context(COUNTRY, LEADER, &[LEADER, Slot::SelfLink])]
    );
}

#[test]
fn clearing_the_links_keeps_the_type() {
    let result = fire_country(&[
        (0x1020, "add", "x0,sp,#0x180"),
        (0x1024, "bl", "#0x8000"),
        (0x1028, "add", "x8,sp,#0x180"),
        (0x102c, "str", "x8,[sp,#0x138]"),
        (0x1030, "add", "x0,sp,#0x100"),
        (0x1034, "bl", "#0x8300"),
    ]);

    assert_eq!(
        contexts(&result, "on_test"),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
}

#[test]
fn different_setters_on_two_paths_give_two_contexts_and_equal_ones_merge() {
    let branching = |second: &'static str| {
        vec![
            (0x1000, "sub", "sp,sp,#0x200"),
            (0x1004, "add", "x0,sp,#0x100"),
            (0x1008, "bl", "#0x8000"),
            (0x100c, "add", "x0,sp,#0x100"),
            (0x1010, "cbz", "x19,#0x101c"),
            (0x1014, "bl", "#0x8100"),
            (0x1018, "b", "#0x1020"),
            (0x101c, "bl", second),
            (0x1020, "add", "x0,sp,#0x10"),
            (0x1024, "adrp", "x1,#0x5000"),
            (0x1028, "bl", "#0x9100"),
            (0x102c, "add", "x1,sp,#0x10"),
            (0x1030, "add", "x2,sp,#0x100"),
            (0x1034, "bl", "#0x9000"),
            (0x1038, "ret", ""),
        ]
    };
    let run = |second| {
        let lines = branching(second);
        on_actions(
            &Program::new()
                .function(&lines)
                .site(0x1000, 0x1034, FIRE)
                .input(),
        )
    };

    assert_eq!(
        contexts(&run("#0x8200"), "on_test"),
        [
            context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink]),
            context(LEADER, Slot::SelfLink, &[Slot::SelfLink]),
        ]
    );
    assert_eq!(
        contexts(&run("#0x8100"), "on_test"),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
}

fn unresolved() -> Context {
    context(Slot::Unresolved, Slot::Unresolved, &[Slot::Unresolved])
}

#[test]
fn one_unfollowed_call_that_receives_the_scope_makes_it_unresolved() {
    let result = fire_country(&[(0x1020, "add", "x0,sp,#0x100"), (0x1024, "bl", "#0x9900")]);

    assert_eq!(contexts(&result, "on_test"), [unresolved()]);
}

#[test]
fn a_scope_stored_outside_a_link_escapes_at_the_next_unfollowed_call() {
    let result = fire_country(&[
        (0x1020, "add", "x8,sp,#0x100"),
        (0x1024, "str", "x8,[sp,#0x20]"),
        (0x1028, "mov", "x0,x19"),
        (0x102c, "bl", "#0x9900"),
    ]);

    assert_eq!(contexts(&result, "on_test"), [unresolved()]);
}

#[test]
fn a_scope_linked_from_an_escaped_scope_escapes_with_it() {
    let lines = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x100"),
        (0x1008, "bl", "#0x8000"),
        (0x100c, "add", "x0,sp,#0x180"),
        (0x1010, "bl", "#0x8000"),
        (0x1014, "add", "x0,sp,#0x180"),
        (0x1018, "bl", "#0x8100"),
        (0x101c, "add", "x8,sp,#0x180"),
        (0x1020, "str", "x8,[sp,#0x138]"),
        (0x1024, "add", "x0,sp,#0x100"),
        (0x1028, "bl", "#0x9900"),
        (0x102c, "add", "x0,sp,#0x10"),
        (0x1030, "adrp", "x1,#0x5000"),
        (0x1034, "bl", "#0x9100"),
        (0x1038, "add", "x1,sp,#0x10"),
        (0x103c, "add", "x2,sp,#0x180"),
        (0x1040, "bl", "#0x9000"),
        (0x1044, "ret", ""),
    ];
    let result = on_actions(
        &Program::new()
            .function(&lines)
            .site(0x1000, 0x1040, FIRE)
            .input(),
    );

    assert_eq!(contexts(&result, "on_test"), [unresolved()]);
}

#[test]
fn a_store_through_an_unknown_pointer_keeps_a_private_scope() {
    let result = fire_country(&[
        (0x1020, "ldr", "x8,[x19]"),
        (0x1024, "str", "x9,[x8]"),
        (0x1028, "mov", "x0,x19"),
        (0x102c, "bl", "#0x9900"),
    ]);

    assert_eq!(
        contexts(&result, "on_test"),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
}

#[test]
fn a_copied_scope_is_unresolved() {
    let result = fire_country(&[
        (0x1020, "add", "x0,sp,#0x100"),
        (0x1024, "mov", "x1,x19"),
        (0x1028, "bl", "#0x8400"),
    ]);

    assert_eq!(contexts(&result, "on_test"), [unresolved()]);
}

#[test]
fn a_scope_that_the_site_does_not_hold_is_unresolved() {
    let mut lines = fires_country(&[]);
    let scope = lines
        .iter()
        .position(|(address, _, _)| *address == 0x10fc)
        .unwrap();
    lines[scope] = (0x10fc, "ldr", "x2,[x19]");
    let result = on_actions(
        &Program::new()
            .function(&lines)
            .site(0x1000, 0x1100, FIRE)
            .input(),
    );

    assert_eq!(contexts(&result, "on_test"), [unresolved()]);
}

#[test]
fn branches_before_the_site_that_exceed_the_path_limit_keep_the_name() {
    let diamonds: Vec<(u64, &str, &str)> = (0..8u64)
        .flat_map(|index| {
            let at = 0x1020 + index * 8;
            let target: &'static str = Box::leak(format!("x19,#0x{:x}", at + 8).into_boxed_str());
            [(at, "cbz", target), (at + 4, "nop", "")]
        })
        .collect();
    let result = fire_country(&diamonds);

    let findings = &result.on_actions["on_test"];
    assert!(findings.unresolved.contains("path-limit"), "{result:?}");
}

#[test]
fn two_branches_that_build_different_names_in_one_string_give_both() {
    let lines = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x100"),
        (0x1008, "bl", "#0x8000"),
        (0x100c, "add", "x0,sp,#0x100"),
        (0x1010, "bl", "#0x8100"),
        (0x1014, "adrp", "x1,#0x5000"),
        (0x1018, "cbz", "x19,#0x1020"),
        (0x101c, "add", "x1,x1,#0x10"),
        (0x1020, "add", "x0,sp,#0x10"),
        (0x1024, "bl", "#0x9100"),
        (0x1028, "add", "x1,sp,#0x10"),
        (0x102c, "add", "x2,sp,#0x100"),
        (0x1030, "bl", "#0x9000"),
        (0x1034, "ret", ""),
    ];
    let result = on_actions(
        &Program::new()
            .function(&lines)
            .site(0x1000, 0x1030, FIRE)
            .input(),
    );

    for name in ["on_test", "on_other"] {
        assert_eq!(
            contexts(&result, name),
            [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])],
            "{name}"
        );
    }
}

#[test]
fn a_string_that_a_call_changes_after_it_is_built_is_not_named() {
    let result = fire_country(&[(0x1020, "add", "x0,sp,#0x10"), (0x1024, "bl", "#0x9900")]);

    assert!(result.on_actions.is_empty());
    assert!(
        result
            .unnamed
            .iter()
            .any(|unnamed| unnamed.reason == "name-not-a-literal")
    );
}

#[test]
fn a_reused_string_slot_keeps_no_stale_name() {
    let lines = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x10"),
        (0x1008, "adrp", "x1,#0x5000"),
        (0x100c, "add", "x1,x1,#0x10"),
        (0x1010, "bl", "#0x9100"),
        (0x1014, "add", "x0,sp,#0x10"),
        (0x1018, "bl", "#0x9108"),
        (0x101c, "add", "x0,sp,#0x100"),
        (0x1020, "bl", "#0x8000"),
        (0x1024, "add", "x0,sp,#0x10"),
        (0x1028, "adrp", "x1,#0x5000"),
        (0x102c, "bl", "#0x9100"),
        (0x1030, "add", "x1,sp,#0x10"),
        (0x1034, "add", "x2,sp,#0x100"),
        (0x1038, "bl", "#0x9000"),
        (0x103c, "ret", ""),
    ];
    let result = on_actions(
        &Program::new()
            .function(&lines)
            .site(0x1000, 0x1038, FIRE)
            .input(),
    );

    assert_eq!(result.on_actions.keys().collect::<Vec<_>>(), ["on_test"]);
}

/// The caller of a forwarder at 0x2000 builds the country scope and the name, and passes them in
/// `x0` and `x1`.
fn forwarder_caller() -> Vec<(u64, &'static str, &'static str)> {
    vec![
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x100"),
        (0x1008, "bl", "#0x8000"),
        (0x100c, "add", "x0,sp,#0x100"),
        (0x1010, "bl", "#0x8100"),
        (0x1014, "add", "x0,sp,#0x10"),
        (0x1018, "adrp", "x1,#0x5000"),
        (0x101c, "bl", "#0x9100"),
        (0x1020, "add", "x0,sp,#0x10"),
        (0x1024, "add", "x1,sp,#0x100"),
        (0x1028, "bl", "#0x2000"),
        (0x102c, "ret", ""),
    ]
}

#[test]
fn a_forwarder_that_passes_its_callers_name_and_scope_gives_the_callers_context() {
    let forwarder = [
        (0x2000, "mov", "x2,x1"),
        (0x2004, "mov", "x1,x0"),
        (0x2008, "b", "#0x9000"),
    ];
    let caller = forwarder_caller();
    let input = Program::new()
        .function(&caller)
        .function(&forwarder)
        .site(0x2000, 0x2008, FIRE)
        .site(0x1000, 0x1028, SiteCall::Forwarded { forwarder: 0 })
        .forwarder(
            0x2000,
            ForwarderKind::Fire {
                name: 0,
                scope: Some(1),
            },
        )
        .input();
    let result = on_actions(&input);

    assert_eq!(
        contexts(&result, "on_test"),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
}

#[test]
fn a_forwarder_that_builds_its_own_scope_gives_its_context_to_the_callers_name() {
    let forwarder = [
        (0x2000, "sub", "sp,sp,#0x200"),
        (0x2004, "mov", "x19,x0"),
        (0x2008, "add", "x0,sp,#0x100"),
        (0x200c, "bl", "#0x8000"),
        (0x2010, "add", "x0,sp,#0x100"),
        (0x2014, "bl", "#0x8200"),
        (0x2018, "mov", "x1,x19"),
        (0x201c, "add", "x2,sp,#0x100"),
        (0x2020, "bl", "#0x9000"),
        (0x2024, "ret", ""),
    ];
    let caller = forwarder_caller();
    let input = Program::new()
        .function(&caller)
        .function(&forwarder)
        .site(0x2000, 0x2020, FIRE)
        .site(0x1000, 0x1028, SiteCall::Forwarded { forwarder: 0 })
        .forwarder(
            0x2000,
            ForwarderKind::Fire {
                name: 0,
                scope: None,
            },
        )
        .input();
    let result = on_actions(&input);

    assert_eq!(
        contexts(&result, "on_test"),
        [context(LEADER, Slot::SelfLink, &[Slot::SelfLink])]
    );
}

#[test]
fn a_forwarder_that_does_not_pass_its_argument_is_refused() {
    let forwarder = [
        (0x2000, "ldr", "x1,[x0]"),
        (0x2004, "mov", "x2,x1"),
        (0x2008, "b", "#0x9000"),
    ];
    let caller = forwarder_caller();
    let input = Program::new()
        .function(&caller)
        .function(&forwarder)
        .site(0x2000, 0x2008, FIRE)
        .site(0x1000, 0x1028, SiteCall::Forwarded { forwarder: 0 })
        .forwarder(
            0x2000,
            ForwarderKind::Fire {
                name: 0,
                scope: Some(1),
            },
        )
        .input();
    let result = on_actions(&input);

    assert!(result.on_actions.is_empty());
    assert_eq!(
        result.unnamed,
        [Unnamed {
            family: Family::OnAction,
            reason: "forwarder-not-verified"
        }]
    );
}

#[test]
fn a_lookup_names_the_list_that_the_same_function_fires() {
    let lines = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x100"),
        (0x1008, "bl", "#0x8000"),
        (0x100c, "add", "x0,sp,#0x100"),
        (0x1010, "bl", "#0x8100"),
        (0x1014, "add", "x0,sp,#0x10"),
        (0x1018, "adrp", "x1,#0x5000"),
        (0x101c, "bl", "#0x9100"),
        (0x1020, "mov", "x0,x19"),
        (0x1024, "add", "x1,sp,#0x10"),
        (0x1028, "bl", "#0x9020"),
        (0x102c, "mov", "x1,x0"),
        (0x1030, "add", "x2,sp,#0x100"),
        (0x1034, "bl", "#0x9010"),
        (0x1038, "ret", ""),
    ];
    let input = Program::new()
        .function(&lines)
        .site(0x1000, 0x1028, SiteCall::Lookup { name: 1 })
        .site(0x1000, 0x1034, SiteCall::FireList { list: 1, scope: 2 })
        .input();
    let result = on_actions(&input);

    assert_eq!(
        contexts(&result, "on_test"),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
    assert!(result.on_actions["on_test"].unresolved.is_empty());
}

/// The database load function compares list names on a chain of branches and stores each list
/// in a separate block. The first store after the first comparison in address order belongs to
/// the second name.
fn pulse_program() -> Program {
    let init = [
        (0x4000, "mov", "x19,x0"),
        (0x4004, "mov", "x0,x20"),
        (0x4008, "adrp", "x1,#0x5000"),
        (0x400c, "bl", "#0x9300"),
        (0x4010, "cbz", "w0,#0x4040"),
        (0x4014, "mov", "x0,x20"),
        (0x4018, "adrp", "x1,#0x5000"),
        (0x401c, "add", "x1,x1,#0x10"),
        (0x4020, "bl", "#0x9300"),
        (0x4024, "cbz", "w0,#0x4034"),
        (0x4028, "ret", ""),
        (0x402c, "nop", ""),
        (0x4030, "nop", ""),
        (0x4034, "str", "x21,[x19,#0x28]"),
        (0x4038, "ret", ""),
        (0x403c, "nop", ""),
        (0x4040, "str", "x21,[x19,#0x30]"),
        (0x4044, "ret", ""),
    ];
    let fire = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x100"),
        (0x1008, "bl", "#0x8000"),
        (0x100c, "add", "x0,sp,#0x100"),
        (0x1010, "bl", "#0x8100"),
        (0x1014, "adrp", "x8,#0x6000"),
        (0x1018, "add", "x8,x8,#0xf00"),
        (0x101c, "ldr", "x8,[x8]"),
        (0x1020, "ldr", "x1,[x8,#0x30]"),
        (0x1024, "add", "x2,sp,#0x100"),
        (0x1028, "bl", "#0x9010"),
        (0x102c, "ret", ""),
    ];
    let mut program = Program::new().function(&init).function(&fire).site(
        0x1000,
        0x1028,
        SiteCall::FireList { list: 1, scope: 2 },
    );
    program.pulse = Some(Pulse {
        init: 0x4000,
        string_compare: BTreeSet::from([STRCMP]),
        instance: INSTANCE,
    });
    program
}

#[test]
fn a_cached_list_is_named_by_the_equal_edge_of_its_comparison() {
    let result = on_actions(&pulse_program().input());

    assert_eq!(
        contexts(&result, "on_test"),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
    assert_eq!(
        result.on_actions["on_other"].unresolved,
        BTreeSet::from(["cached-list-not-fired"])
    );
}

#[test]
fn a_cached_list_without_a_comparison_is_not_named() {
    let mut program = pulse_program();
    program.pulse.as_mut().unwrap().string_compare.clear();
    let result = on_actions(&program.input());

    assert!(result.on_actions.is_empty());
    assert!(
        result
            .unnamed
            .iter()
            .any(|unnamed| unnamed.reason == "cached-list-unknown")
    );
}

/// A rule-set function at 0x3000 that builds a country scope and evaluates the rule at `offset`
/// from its receiver. The offset is reached through a shifted add, as the compiler writes large
/// offsets.
fn rule_function(offset: &'static str) -> Vec<(u64, &'static str, &'static str)> {
    vec![
        (0x3000, "sub", "sp,sp,#0x200"),
        (0x3004, "mov", "x19,x0"),
        (0x3008, "add", "x0,sp,#0x100"),
        (0x300c, "bl", "#0x8000"),
        (0x3010, "add", "x0,sp,#0x100"),
        (0x3014, "bl", "#0x8100"),
        (0x3018, "add", "x0,x19,#0x9,lsl#12"),
        (0x301c, "sub", "x0,x0,#0x9,lsl#12"),
        (0x3020, "add", offset),
        (0x3024, "add", "x1,sp,#0x100"),
        (0x3028, "bl", "#0x9030"),
        (0x302c, "ret", ""),
    ]
}

fn rules(offset: &'static str, call: SiteCall, owned: bool) -> CallbacksResult {
    let lines = rule_function(offset);
    let mut program = Program::new().function(&lines).site(0x3000, 0x3028, call);
    if owned {
        program.rule_owners.insert(0x3000);
    }
    analyze(&program.input(), Family::GameRule).unwrap()
}

fn scripted<'r>(result: &'r CallbacksResult, name: &str) -> &'r Findings {
    &result.rules[&(name.to_owned(), RuleFamily::Scripted)]
}

const SCRIPTED_RULE: SiteCall = SiteCall::Rule {
    family: RuleFamily::Scripted,
    rule: 0,
    scope: 1,
};

#[test]
fn a_rule_is_named_by_its_offset_in_the_rule_set() {
    let result = rules("x0,x0,#0xc0", SCRIPTED_RULE, true);

    let findings = scripted(&result, "can_b");
    assert_eq!(
        findings.contexts.iter().cloned().collect::<Vec<_>>(),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
    assert_eq!(
        scripted(&result, "can_a").unresolved,
        BTreeSet::from(["no-site"])
    );
}

#[test]
fn a_weighted_rule_uses_its_own_array() {
    let weighted = SiteCall::Rule {
        family: RuleFamily::Weighted,
        rule: 0,
        scope: 1,
    };
    let result = rules("x0,x0,#0x9cc0", weighted, true);

    let weight = &result.rules[&("weight_c".to_owned(), RuleFamily::Weighted)];
    assert!(!weight.contexts.is_empty());
}

#[test]
fn a_rule_offset_that_is_not_a_rule_or_outside_the_rule_set_is_not_named() {
    let misaligned = rules("x0,x0,#0xc8", SCRIPTED_RULE, true);
    let outside = rules("x0,x0,#0xc0", SCRIPTED_RULE, false);

    assert!(scripted(&misaligned, "can_b").contexts.is_empty());
    assert_eq!(misaligned.unnamed[0].reason, "rule-offset");
    assert_eq!(outside.unnamed[0].reason, "rule-outside-the-rule-set");
}

#[test]
fn a_rule_forwarder_is_checked_with_a_probe_and_named_by_its_callers_constant() {
    let forwarder = [
        (0x3400, "sub", "sp,sp,#0x200"),
        (0x3404, "mov", "x19,x0"),
        (0x3408, "mov", "w20,w1"),
        (0x340c, "add", "x0,sp,#0x100"),
        (0x3410, "bl", "#0x8000"),
        (0x3414, "add", "x0,sp,#0x100"),
        (0x3418, "bl", "#0x8100"),
        (0x341c, "mov", "w8,#0xc0"),
        (0x3420, "umaddl", "x0,w20,w8,x19"),
        (0x3424, "add", "x1,sp,#0x100"),
        (0x3428, "bl", "#0x9030"),
        (0x342c, "ret", ""),
    ];
    let caller = [
        (0x3800, "mov", "w1,#1"),
        (0x3804, "bl", "#0x3400"),
        (0x3808, "ret", ""),
    ];
    let mut program = Program::new()
        .function(&forwarder)
        .function(&caller)
        .site(0x3400, 0x3428, SCRIPTED_RULE)
        .site(0x3800, 0x3804, SiteCall::Forwarded { forwarder: 0 })
        .forwarder(
            0x3400,
            ForwarderKind::Rule {
                family: RuleFamily::Scripted,
                enumeration: 1,
            },
        );
    program.rule_owners.extend([0x3400, 0x3800]);
    let result = analyze(&program.input(), Family::GameRule).unwrap();

    assert_eq!(
        scripted(&result, "can_b")
            .contexts
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
}

#[test]
fn a_wrong_rule_stride_names_nothing_that_exists() {
    let mut input = {
        let lines = rule_function("x0,x0,#0xc0");
        let mut program = Program::new()
            .function(&lines)
            .site(0x3000, 0x3028, SCRIPTED_RULE);
        program.rule_owners.insert(0x3000);
        program.input()
    };
    input.layout.scripted_rules.stride = 0x100;
    let result = analyze(&input, Family::GameRule).unwrap();

    assert!(
        result
            .rules
            .values()
            .all(|findings| findings.contexts.is_empty())
    );
}

#[test]
fn without_a_site_of_the_family_the_method_refuses_its_input() {
    let lines = fires_country(&[]);
    let input = Program::new()
        .function(&lines)
        .site(0x1000, 0x1100, FIRE)
        .input();

    assert!(analyze(&input, Family::GameRule).is_err());
}

#[test]
fn without_the_string_constructor_no_name_is_given() {
    let lines = fires_country(&[]);
    let mut input = Program::new()
        .function(&lines)
        .site(0x1000, 0x1100, FIRE)
        .input();
    input.strings.from_literal.clear();
    let result = on_actions(&input);

    assert!(result.on_actions.is_empty());
    assert!(!result.unnamed.is_empty());
}

/// The load function spills the address of a cached list's field to a stack slot and stores the
/// list through it on the equal edge. `between` runs after the spill.
fn spilled_pulse(between: Rows<'_>) -> CallbacksResult {
    let mut init = vec![
        (0x4000, "sub", "sp,sp,#0x100"),
        (0x4004, "add", "x8,x0,#0x28"),
        (0x4008, "str", "x8,[sp,#0x30]"),
    ];
    init.extend_from_slice(between);
    let end = init.last().map_or(0x4000, |(address, _, _)| *address);
    init.extend(
        (end + 4..0x4040)
            .step_by(4)
            .map(|address| (address, "nop", "")),
    );
    init.extend([
        (0x4040, "mov", "x0,x20"),
        (0x4044, "adrp", "x1,#0x5000"),
        (0x4048, "bl", "#0x9300"),
        (0x404c, "cbz", "w0,#0x4054"),
        (0x4050, "ret", ""),
        (0x4054, "ldr", "x8,[sp,#0x30]"),
        (0x4058, "str", "x21,[x8]"),
        (0x405c, "ret", ""),
    ]);
    let fire = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x100"),
        (0x1008, "bl", "#0x8000"),
        (0x100c, "adrp", "x8,#0x6000"),
        (0x1010, "add", "x8,x8,#0xf00"),
        (0x1014, "ldr", "x8,[x8]"),
        (0x1018, "ldr", "x1,[x8,#0x28]"),
        (0x101c, "add", "x2,sp,#0x100"),
        (0x1020, "bl", "#0x9010"),
        (0x1024, "ret", ""),
    ];
    let mut program = Program::new().function(&init).function(&fire).site(
        0x1000,
        0x1020,
        SiteCall::FireList { list: 1, scope: 2 },
    );
    program.pulse = Some(Pulse {
        init: 0x4000,
        string_compare: BTreeSet::from([STRCMP]),
        instance: INSTANCE,
    });
    on_actions(&program.input())
}

#[test]
fn a_spilled_field_address_joins_its_cached_list() {
    let result = spilled_pulse(&[(0x400c, "mov", "x0,x19"), (0x4010, "bl", "#0x9900")]);

    assert_eq!(
        contexts(&result, "on_test"),
        [context(Slot::NotSet, Slot::SelfLink, &[Slot::SelfLink])]
    );
}

#[test]
fn a_call_that_receives_a_lower_stack_address_forgets_the_spill() {
    let result = spilled_pulse(&[(0x400c, "add", "x0,sp,#0x10"), (0x4010, "bl", "#0x9900")]);

    assert!(result.on_actions.is_empty());
}

#[test]
fn a_rule_offset_in_a_register_names_the_rule() {
    let mut lines = rule_function("x0,x19,x8");
    lines[7] = (0x301c, "mov", "w8,#0xc0");
    lines[6] = (0x3018, "nop", "");
    let mut program = Program::new()
        .function(&lines)
        .site(0x3000, 0x3028, SCRIPTED_RULE);
    program.rule_owners.insert(0x3000);
    let result = analyze(&program.input(), Family::GameRule).unwrap();

    assert!(!scripted(&result, "can_b").contexts.is_empty());
}

#[test]
fn a_stack_pointer_move_keeps_the_objects_that_were_built_before_it() {
    let lines = [
        (0x1000, "stp", "x29,x30,[sp,#-0x10]!"),
        (0x1004, "sub", "sp,sp,#0x200"),
        (0x1008, "add", "x0,sp,#0x100"),
        (0x100c, "bl", "#0x8000"),
        (0x1010, "add", "x0,sp,#0x100"),
        (0x1014, "bl", "#0x8100"),
        (0x1018, "add", "x0,sp,#0x10"),
        (0x101c, "adrp", "x1,#0x5000"),
        (0x1020, "bl", "#0x9100"),
        (0x1024, "sub", "sp,sp,#0x20"),
        (0x1028, "add", "x1,sp,#0x30"),
        (0x102c, "add", "x2,sp,#0x120"),
        (0x1030, "bl", "#0x9000"),
        (0x1034, "ret", ""),
    ];
    let result = on_actions(
        &Program::new()
            .function(&lines)
            .site(0x1000, 0x1030, FIRE)
            .input(),
    );

    assert_eq!(
        contexts(&result, "on_test"),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
}

#[test]
fn a_site_without_a_literal_name_is_counted_once() {
    let result = fire_country(&[(0x1020, "add", "x0,sp,#0x10"), (0x1024, "bl", "#0x9900")]);

    assert_eq!(result.unnamed.len(), 1);
}

#[test]
fn a_frame_register_keeps_its_objects_when_the_stack_pointer_moves_by_an_unknown_amount() {
    let lines = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "mov", "x19,sp"),
        (0x1008, "mov", "x9,sp"),
        (0x100c, "sub", "x22,x9,x8"),
        (0x1010, "mov", "sp,x22"),
        (0x1014, "add", "x0,x19,#0x100"),
        (0x1018, "bl", "#0x8000"),
        (0x101c, "add", "x0,x19,#0x100"),
        (0x1020, "bl", "#0x8100"),
        (0x1024, "add", "x0,x19,#0x10"),
        (0x1028, "adrp", "x1,#0x5000"),
        (0x102c, "bl", "#0x9100"),
        (0x1030, "add", "x1,x19,#0x10"),
        (0x1034, "add", "x2,x19,#0x100"),
        (0x1038, "bl", "#0x9000"),
        (0x103c, "ret", ""),
    ];
    let result = on_actions(
        &Program::new()
            .function(&lines)
            .site(0x1000, 0x1038, FIRE)
            .input(),
    );

    assert_eq!(
        contexts(&result, "on_test"),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
}

#[test]
fn a_field_address_formed_by_write_back_joins_its_cached_list() {
    let mut init = vec![
        (0x4000, "sub", "sp,sp,#0x100"),
        (0x4004, "mov", "x8,x0"),
        (0x4008, "str", "q0,[x8,#0x28]!"),
        (0x400c, "str", "x8,[sp,#0x30]"),
    ];
    init.extend(
        (0x4010..0x4040)
            .step_by(4)
            .map(|address| (address, "nop", "")),
    );
    init.extend([
        (0x4040, "mov", "x0,x20"),
        (0x4044, "adrp", "x1,#0x5000"),
        (0x4048, "bl", "#0x9300"),
        (0x404c, "cbz", "w0,#0x4054"),
        (0x4050, "ret", ""),
        (0x4054, "ldr", "x8,[sp,#0x30]"),
        (0x4058, "str", "x21,[x8]"),
        (0x405c, "ret", ""),
    ]);
    let mut program = Program::new().function(&init);
    program.pulse = Some(Pulse {
        init: 0x4000,
        string_compare: BTreeSet::from([STRCMP]),
        instance: INSTANCE,
    });
    let input = program.input();

    assert_eq!(pulse_names(&input), BTreeMap::from([(0x28, ON_TEST)]));
}

#[test]
fn a_site_reached_only_through_a_jump_table_is_followed() {
    let lines = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x100"),
        (0x1008, "bl", "#0x8000"),
        (0x100c, "add", "x0,sp,#0x100"),
        (0x1010, "bl", "#0x8100"),
        (0x1014, "add", "x0,sp,#0x10"),
        (0x1018, "adrp", "x1,#0x5000"),
        (0x101c, "bl", "#0x9100"),
        (0x1020, "adr", "x9,#0x102c"),
        (0x1024, "br", "x9"),
        (0x1028, "ret", ""),
        (0x102c, "add", "x1,sp,#0x10"),
        (0x1030, "add", "x2,sp,#0x100"),
        (0x1034, "bl", "#0x9000"),
        (0x1038, "ret", ""),
    ];
    let result = on_actions(
        &Program::new()
            .function(&lines)
            .site(0x1000, 0x1034, FIRE)
            .input(),
    );

    assert_eq!(
        contexts(&result, "on_test"),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])]
    );
}

#[test]
fn an_unfollowed_instruction_that_starts_with_b_clears_its_destination() {
    let lines = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x100"),
        (0x1008, "bl", "#0x8000"),
        (0x100c, "adrp", "x1,#0x5000"),
        (0x1010, "bic", "x1,x1,x2"),
        (0x1014, "add", "x0,sp,#0x10"),
        (0x1018, "bl", "#0x9100"),
        (0x101c, "add", "x1,sp,#0x10"),
        (0x1020, "add", "x2,sp,#0x100"),
        (0x1024, "bl", "#0x9000"),
        (0x1028, "ret", ""),
    ];
    let result = on_actions(
        &Program::new()
            .function(&lines)
            .site(0x1000, 0x1024, FIRE)
            .input(),
    );

    assert!(result.on_actions.is_empty());
}

#[test]
fn a_string_that_only_one_branch_builds_names_only_that_branch() {
    let lines = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "add", "x0,sp,#0x100"),
        (0x1008, "bl", "#0x8000"),
        (0x100c, "add", "x0,sp,#0x100"),
        (0x1010, "cbz", "x19,#0x1028"),
        (0x1014, "bl", "#0x8100"),
        (0x1018, "add", "x0,sp,#0x10"),
        (0x101c, "adrp", "x1,#0x5000"),
        (0x1020, "bl", "#0x9100"),
        (0x1024, "b", "#0x1034"),
        (0x1028, "bl", "#0x8200"),
        (0x102c, "add", "x0,sp,#0x10"),
        (0x1030, "bl", "#0x9900"),
        (0x1034, "add", "x1,sp,#0x10"),
        (0x1038, "add", "x2,sp,#0x100"),
        (0x103c, "bl", "#0x9000"),
        (0x1040, "ret", ""),
    ];
    let result = on_actions(
        &Program::new()
            .function(&lines)
            .site(0x1000, 0x103c, FIRE)
            .input(),
    );

    assert_eq!(
        contexts(&result, "on_test"),
        [context(COUNTRY, Slot::SelfLink, &[Slot::SelfLink])],
        "the leader context belongs to a name that the method does not know"
    );
    assert!(
        result
            .unnamed
            .iter()
            .any(|unnamed| unnamed.reason == "name-not-a-literal")
    );
    assert!(
        result.on_actions["on_test"]
            .unresolved
            .contains("context-not-attributed")
    );
}

#[test]
fn a_setter_that_passes_the_scope_to_an_unfollowed_call_leaves_it_unresolved() {
    let result = fire_country(&[(0x1020, "add", "x0,sp,#0x100"), (0x1024, "bl", "#0x8500")]);

    assert_eq!(contexts(&result, "on_test")[0].this, Slot::Unresolved);
}
