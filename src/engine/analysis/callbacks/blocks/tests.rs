//! Authored-input tests of the block method, with negative controls.
use std::collections::BTreeMap;

use super::super::tests::{
    COPY, COPY_CONSTRUCT, COPY_INTERNAL, COUNTRY, DESTROY, FACTORY, FACTORY_FROM, FRESH, LEADER,
    PASSES_ON, Rows, SET_COUNTRY, SET_LEADER, layout, rows, scope_code,
};
use super::*;
use crate::engine::analysis::callbacks::{Context, Slot};
use crate::engine::analysis::declarations::number;

// Rows call the evaluator at 0x9400, the weight evaluator at 0x9408, a tooltip builder at
// 0x9410, a function that never returns at 0x9420 and an unknown function at 0x9900. Owner
// methods evaluate `potential` at `this + 0x40`, `allow` at `this + 0x48` and `ai_weight` at
// `this + 0x50`.
const EVALUATE: u64 = 0x9400;
const WEIGHT: u64 = 0x9408;
const TOOLTIP: u64 = 0x9410;
const NEVER_RETURNS: u64 = 0x9420;
const OWNER: &str = "COwner";
const POTENTIAL: i64 = 0x40;
const ALLOW: i64 = 0x48;
const AI_WEIGHT: i64 = 0x50;

const COUNTRY_TYPE: (&str, &str) = ("bl", "#0x8100");
const LEADER_TYPE: (&str, &str) = ("bl", "#0x8200");
const NO_TYPE: (&str, &str) = ("nop", "");

type Line = (u64, &'static str, &'static str);

/// Owner methods and other functions with type pointers, whose evaluator calls and calls through a
/// register are sites, and other functions. A direct call to a function of the program is a caller
/// of it.
struct Program {
    functions: BTreeMap<u64, Vec<Instruction>>,
    type_pointers: BTreeMap<u64, TypePointers>,
    arguments: BTreeMap<u64, usize>,
    call_arguments: BTreeMap<u64, usize>,
    receivers: BTreeSet<u64>,
    readers: BTreeSet<u64>,
    instances: BTreeMap<u64, u64>,
    words: BTreeMap<u64, u64>,
    factories: BTreeMap<u64, Vec<Instruction>>,
    offset_getters: BTreeMap<u64, i64>,
}

impl Program {
    fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
            type_pointers: BTreeMap::new(),
            arguments: BTreeMap::new(),
            call_arguments: BTreeMap::new(),
            receivers: BTreeSet::new(),
            readers: BTreeSet::new(),
            instances: BTreeMap::new(),
            words: BTreeMap::new(),
            factories: BTreeMap::new(),
            offset_getters: BTreeMap::new(),
        }
    }

    /// A function that receives a scope and changes no type or link.
    fn reader(mut self, function: u64) -> Self {
        self.readers.insert(function);
        self
    }

    /// The instance pointer at `pointer` holds an object with the vtable at `point`, and `words`
    /// are the loaded words that a virtual call on it reads.
    fn instance(mut self, pointer: u64, point: u64, words: &[(u64, u64)]) -> Self {
        self.instances.insert(pointer, point);
        self.words.extend(words.iter().copied());
        self
    }

    /// The call through a pointer at `call` reads `count` argument registers.
    fn call_arguments(mut self, call: u64, count: usize) -> Self {
        self.call_arguments.insert(call, count);
        self
    }

    /// A function that receives a scope, which the pass runs on the path.
    fn receiver(mut self, lines: Rows<'_>) -> Self {
        self.receivers.insert(lines[0].0);
        self.function(lines)
    }

    /// `function` reads `count` argument registers.
    fn arguments(mut self, function: u64, count: usize) -> Self {
        self.arguments.insert(function, count);
        self
    }

    /// The function at `function` is an offset getter that returns its receiver plus `offset`.
    fn getter(mut self, function: u64, offset: i64) -> Self {
        self.offset_getters.insert(function, offset);
        self
    }

    /// A function that builds a scope in the object that `x8` addresses.
    fn factory(mut self, lines: Rows<'_>) -> Self {
        self.factories.insert(lines[0].0, rows(lines));
        self
    }

    /// A method of the owner, whose receiver points at the owner's item.
    fn method(self, lines: Rows<'_>) -> Self {
        let pointers = TypePointers {
            method_of: Some(OWNER.into()),
            registers: BTreeMap::from([(0, OWNER.into())]),
            members: BTreeMap::new(),
        };
        self.pointing(pointers, lines)
    }

    /// A function whose registers lead to owners as `pointers` states.
    fn pointing(mut self, pointers: TypePointers, lines: Rows<'_>) -> Self {
        self.type_pointers.insert(lines[0].0, pointers);
        self.function(lines)
    }

    fn function(mut self, lines: Rows<'_>) -> Self {
        self.functions.insert(lines[0].0, rows(lines));
        self
    }

    fn analyze(self) -> BlockEntries {
        let direct_calls = |rows: &[Instruction]| -> Vec<(u64, u64)> {
            rows.iter()
                .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
                .filter_map(|row| Some((row.address, number(&row.operands)?)))
                .collect()
        };

        let evaluators = BTreeMap::from([
            (EVALUATE, BlockFamily::Trigger),
            (WEIGHT, BlockFamily::Weight),
        ]);
        let mut sites = Vec::new();
        let mut callers: BTreeMap<u64, Vec<CallSite>> = BTreeMap::new();
        for (&function, rows) in &self.functions {
            if self.type_pointers.contains_key(&function) {
                for row in rows.iter().filter(|row| row.operation == "blr") {
                    let register = names::split(&row.operands)[0];
                    sites.push(EvaluationSite {
                        address: row.address,
                        function,
                        call: EvaluationCall::Register(
                            crate::engine::analysis::decode::general_register(register).unwrap(),
                        ),
                    });
                }
            }
            for (address, target) in direct_calls(rows) {
                let direct = evaluators.contains_key(&target) || target == TOOLTIP;
                if direct && self.type_pointers.contains_key(&function) {
                    sites.push(EvaluationSite {
                        address,
                        function,
                        call: EvaluationCall::Direct(target),
                    });
                }
                if self.functions.contains_key(&target) {
                    callers
                        .entry(target)
                        .or_default()
                        .push(CallSite { address, function });
                }
            }
        }

        analyze_blocks(&BlockInput {
            sites,
            evaluators,
            evaluation_slots: BTreeMap::from([
                (0x10, BlockFamily::Trigger),
                (0x18, BlockFamily::Trigger),
                (0x20, BlockFamily::Trigger),
                (0x48, BlockFamily::Effect),
            ]),
            tooltip_builders: BTreeMap::from([(TOOLTIP, BlockFamily::Trigger)]),
            tooltip_slots: BTreeMap::from([(0x58, BlockFamily::Trigger)]),
            offset_getters: self.offset_getters,
            type_pointers: self.type_pointers,
            scope_users: BTreeSet::from([TOOLTIP]),
            functions: self.functions,
            callers,
            scope_code: scope_code()
                .into_iter()
                .chain(self.factories.values().flatten().cloned())
                .collect(),
            scope_functions: ScopeFunctions {
                fresh_constructors: BTreeSet::from([FRESH]),
                setters: BTreeSet::from([SET_COUNTRY, SET_LEADER, PASSES_ON]),
                copy_constructors: BTreeSet::from([COPY_CONSTRUCT]),
                internal_copies: BTreeSet::from([COPY_INTERNAL]),
                copies: BTreeSet::from([COPY]),
                factories: self.factories.into_keys().collect(),
                destructors: BTreeSet::from([DESTROY]),
                readers: self.readers,
            },
            strings: StringFunctions::default(),
            data: ReadOnlyData::default().with_words(&self.words),
            layout: layout(),
            scope_names: None,
            arguments: self.arguments,
            call_arguments: self.call_arguments,
            ignores_x8: BTreeSet::new(),
            instances: self.instances,
            receivers: self.receivers,
            never_return: BTreeSet::from([NEVER_RETURNS]),
        })
    }
}

/// The start of a function at `base` that keeps its receiver in `x21` and builds scope A at
/// sp+0x100 with `typed`. The next instruction is at `base + 0x18`.
fn builds_a(base: u64, typed: (&'static str, &'static str)) -> Vec<Line> {
    vec![
        (base, "sub", "sp,sp,#0x200"),
        (base + 0x4, "mov", "x21,x0"),
        (base + 0x8, "add", "x0,sp,#0x100"),
        (base + 0xc, "bl", "#0x8000"),
        (base + 0x10, "add", "x0,sp,#0x100"),
        (base + 0x14, typed.0, typed.1),
    ]
}

/// A call at `at` to `callee` with the receiver and scope A.
fn calls_with_a(at: u64, callee: &'static str) -> [Line; 3] {
    [
        (at, "mov", "x0,x21"),
        (at + 4, "add", "x1,sp,#0x100"),
        (at + 8, "bl", callee),
    ]
}

/// A function at `base` that builds scope A with `typed`, calls `callee` with it and returns.
fn caller(base: u64, typed: (&'static str, &'static str), callee: &'static str) -> Vec<Line> {
    let mut lines = builds_a(base, typed);
    lines.extend(calls_with_a(base + 0x18, callee));
    lines.push((base + 0x24, "ret", ""));
    lines
}

/// A wrapper at `base` that evaluates the block at `this + offset` with its scope parameter.
fn wrapper(base: u64, offset: &'static str) -> Vec<Line> {
    vec![
        (base, "mov", "x19,x0"),
        (base + 0x4, "mov", "x20,x1"),
        (base + 0x8, "add", offset),
        (base + 0xc, "mov", "x1,x20"),
        (base + 0x10, "bl", "#0x9400"),
        (base + 0x14, "ret", ""),
    ]
}

const POTENTIAL_OF_X19: &str = "x0,x19,#0x40";
const ALLOW_OF_X19: &str = "x0,x19,#0x48";

/// The owner's block at `offset`: the weight evaluator evaluates `ai_weight`, and the evaluator
/// the others.
fn block(offset: i64) -> Block {
    let family = match offset {
        AI_WEIGHT => BlockFamily::Weight,
        _ => BlockFamily::Trigger,
    };
    Block {
        owner: OWNER.into(),
        offset,
        family,
    }
}

fn contexts(result: &BlockEntries, offset: i64) -> Vec<Context> {
    block_contexts(result, &block(offset))
}

fn block_contexts(result: &BlockEntries, block: &Block) -> Vec<Context> {
    result
        .blocks
        .get(block)
        .map(|findings| findings.contexts.iter().cloned().collect())
        .unwrap_or_default()
}

/// The owner's effect block at `offset`.
fn effect(offset: i64) -> Block {
    Block {
        family: BlockFamily::Effect,
        ..block(offset)
    }
}

fn unresolved(result: &BlockEntries, offset: i64) -> Vec<&'static str> {
    result
        .blocks
        .get(&block(offset))
        .map(|findings| findings.unresolved.iter().copied().collect())
        .unwrap_or_default()
}

/// A scope of type `this` whose links all point back to itself.
fn fresh(this: Slot) -> Context {
    Context {
        this,
        root: Slot::SelfLink,
        from: vec![Slot::SelfLink],
        prev: vec![Slot::SelfLink],
    }
}

fn unreadable() -> Context {
    Context {
        this: Slot::Unresolved,
        root: Slot::Unresolved,
        from: vec![Slot::Unresolved],
        prev: vec![Slot::Unresolved],
    }
}

#[test]
fn a_scope_that_the_method_builds_reaches_its_block() {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "add", "x0,x21,#0x40"),
        (0x101c, "add", "x1,sp,#0x100"),
        (0x1020, "bl", "#0x9400"),
        (0x1024, "ret", ""),
    ]);
    let result = Program::new().method(&lines).analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert!(result.unattributed.is_empty());
}

#[test]
fn a_weight_evaluation_reaches_its_block_and_keeps_the_scope_known_for_the_next() {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "add", "x0,x21,#0x50"),
        (0x101c, "add", "x1,sp,#0x100"),
        (0x1020, "bl", "#0x9408"),
        (0x1024, "add", "x0,x21,#0x40"),
        (0x1028, "add", "x1,sp,#0x100"),
        (0x102c, "bl", "#0x9400"),
        (0x1030, "ret", ""),
    ]);
    let result = Program::new().method(&lines).analyze();

    assert_eq!(contexts(&result, AI_WEIGHT), [fresh(COUNTRY)]);
    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
}

#[test]
fn a_weight_wrapper_takes_the_scope_that_its_caller_built() {
    let wrapper = [
        (0x2000, "mov", "x19,x0"),
        (0x2004, "mov", "x20,x1"),
        (0x2008, "add", "x0,x19,#0x50"),
        (0x200c, "mov", "x1,x20"),
        (0x2010, "bl", "#0x9408"),
        (0x2014, "ret", ""),
    ];
    let result = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
        .method(&wrapper)
        .analyze();

    assert_eq!(contexts(&result, AI_WEIGHT), [fresh(COUNTRY)]);
    assert!(unresolved(&result, AI_WEIGHT).is_empty());
}

#[test]
fn a_scope_that_a_factory_returns_reaches_its_block() {
    let lines = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "mov", "x21,x0"),
        (0x1008, "add", "x8,sp,#0x100"),
        (0x100c, "bl", "#0x8d00"),
        (0x1010, "add", "x0,x21,#0x40"),
        (0x1014, "add", "x1,sp,#0x100"),
        (0x1018, "bl", "#0x9400"),
        (0x101c, "ret", ""),
    ];
    let followed = Program::new().factory(FACTORY).method(&lines).analyze();
    let unfollowed = Program::new().method(&lines).analyze();

    assert_eq!(
        contexts(&followed, POTENTIAL),
        [Context {
            from: FACTORY_FROM.to_vec(),
            ..fresh(COUNTRY)
        }]
    );
    assert_eq!(contexts(&unfollowed, POTENTIAL), [unreadable()]);
}

#[test]
fn a_block_that_is_not_this_plus_an_offset_is_unattributed() {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "ldr", "x0,[x21,#0x40]"),
        (0x101c, "add", "x1,sp,#0x100"),
        (0x1020, "bl", "#0x9400"),
        (0x1024, "ret", ""),
    ]);
    let result = Program::new().method(&lines).analyze();

    assert!(result.blocks.is_empty());
    assert_eq!(result.unattributed, BTreeMap::from([(OWNER.into(), 1)]));
}

#[test]
fn two_branches_that_select_different_blocks_receive_their_contexts() {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "cbz", "x2,#0x1024"),
        (0x101c, "add", "x0,x21,#0x40"),
        (0x1020, "b", "#0x1028"),
        (0x1024, "add", "x0,x21,#0x48"),
        (0x1028, "add", "x1,sp,#0x100"),
        (0x102c, "bl", "#0x9400"),
        (0x1030, "ret", ""),
    ]);
    let result = Program::new().method(&lines).analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert_eq!(contexts(&result, ALLOW), [fresh(COUNTRY)]);
    assert!(result.unattributed.is_empty());
}

#[test]
fn a_scope_parameter_takes_the_scope_that_the_caller_built_and_linked() {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "add", "x0,sp,#0x180"),
        (0x101c, "bl", "#0x8000"),
        (0x1020, "add", "x0,sp,#0x180"),
        (0x1024, "bl", "#0x8200"),
        (0x1028, "add", "x8,sp,#0x180"),
        (0x102c, "str", "x8,[sp,#0x138]"), // A.from = B
    ]);
    lines.extend(calls_with_a(0x1030, "#0x2000"));
    lines.push((0x103c, "ret", ""));
    let result = Program::new()
        .function(&lines)
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .analyze();

    assert_eq!(
        contexts(&result, POTENTIAL),
        [Context {
            from: vec![LEADER, Slot::SelfLink],
            ..fresh(COUNTRY)
        }]
    );
    assert!(unresolved(&result, POTENTIAL).is_empty());
}

#[test]
fn a_scope_parameter_with_no_caller_has_no_context() {
    let result = Program::new()
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .analyze();

    assert!(contexts(&result, POTENTIAL).is_empty());
    assert_eq!(unresolved(&result, POTENTIAL), ["no-caller"]);
}

/// A wrapper at 0x2000 that runs `before` on its scope in `x20` and then evaluates `potential`.
fn wrapper_doing(before: &[Line]) -> Vec<Line> {
    let mut lines = vec![(0x2000, "mov", "x19,x0"), (0x2004, "mov", "x20,x1")];
    lines.extend_from_slice(before);
    let next = lines.last().unwrap().0 + 4;
    lines.extend([
        (next, "add", "x0,x19,#0x40"),
        (next + 4, "mov", "x1,x20"),
        (next + 8, "bl", "#0x9400"),
        (next + 12, "ret", ""),
    ]);
    lines
}

#[test]
fn a_setter_in_the_wrapper_gives_the_type_that_the_block_receives() {
    let before = [(0x2008, "mov", "x0,x20"), (0x200c, "bl", "#0x8200")];
    let result = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
        .method(&wrapper_doing(&before))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(LEADER)]);
}

#[test]
fn a_scope_that_the_wrapper_passes_to_an_unknown_call_is_unresolved() {
    let before = [(0x2008, "mov", "x0,x20"), (0x200c, "bl", "#0x9900")];
    let result = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
        .method(&wrapper_doing(&before))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [unreadable()]);
}

/// A caller at 0x1000 that links scope R, a leader, as A's root and scope C, a country, as R's
/// prev, then calls 0x2000 with A.
fn links_root_with_its_own_prev() -> Vec<Line> {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "add", "x0,sp,#0x180"),
        (0x101c, "bl", "#0x8000"),
        (0x1020, "add", "x0,sp,#0x180"),
        (0x1024, "bl", "#0x8200"),
        (0x1028, "add", "x0,sp,#0x80"),
        (0x102c, "bl", "#0x8000"),
        (0x1030, "add", "x0,sp,#0x80"),
        (0x1034, "bl", "#0x8100"),
        (0x1038, "add", "x8,sp,#0x180"),
        (0x103c, "str", "x8,[sp,#0x130]"), // A.root = R
        (0x1040, "add", "x8,sp,#0x80"),
        (0x1044, "str", "x8,[sp,#0x1c0]"), // R.prev = C
    ]);
    lines.extend(calls_with_a(0x1048, "#0x2000"));
    lines.push((0x1054, "ret", ""));
    lines
}

#[test]
fn a_link_that_the_wrapper_writes_is_read_through_the_linked_object() {
    let before = [
        (0x2008, "ldr", "x8,[x20,#0x30]"),
        (0x200c, "str", "x8,[x20,#0x40]"), // A.prev = A.root
    ];
    let result = Program::new()
        .function(&links_root_with_its_own_prev())
        .method(&wrapper_doing(&before))
        .analyze();

    assert_eq!(
        contexts(&result, POTENTIAL),
        [Context {
            this: COUNTRY,
            root: LEADER,
            from: vec![Slot::SelfLink],
            prev: vec![LEADER, COUNTRY, Slot::SelfLink],
        }]
    );
}

#[test]
fn link_writes_on_two_paths_of_the_wrapper_give_two_contexts() {
    let before = [
        (0x2008, "cbz", "x2,#0x2014"),
        (0x200c, "ldr", "x8,[x20,#0x30]"),
        (0x2010, "str", "x8,[x20,#0x40]"), // A.prev = A.root
    ];
    let result = Program::new()
        .function(&links_root_with_its_own_prev())
        .method(&wrapper_doing(&before))
        .analyze();
    let unchanged = Context {
        this: COUNTRY,
        root: LEADER,
        from: vec![Slot::SelfLink],
        prev: vec![Slot::SelfLink],
    };
    let linked = Context {
        prev: vec![LEADER, COUNTRY, Slot::SelfLink],
        ..unchanged.clone()
    };

    assert_eq!(contexts(&result, POTENTIAL), [linked, unchanged]);
}

/// A wrapper that runs `test` on its scope, which branches past a leader setter to 0x201c,
/// before it evaluates `potential`.
fn sets_leader_when(test: [Line; 3]) -> Vec<Line> {
    let mut before = test.to_vec();
    before.extend([(0x2014, "mov", "x0,x20"), (0x2018, "bl", "#0x8200")]);
    wrapper_doing(&before)
}

#[test]
fn the_callers_type_selects_a_branch_in_the_wrapper() {
    let wrapper = sets_leader_when([
        (0x2008, "ldr", "x8,[x20,#0x8]"),
        (0x200c, "cmp", "x8,#4"),
        (0x2010, "b.ne", "#0x201c"),
    ]);
    let country = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
        .method(&wrapper)
        .analyze();
    let untyped = Program::new()
        .function(&caller(0x1000, NO_TYPE, "#0x2000"))
        .method(&wrapper)
        .analyze();

    assert_eq!(contexts(&country, POTENTIAL), [fresh(LEADER)]);
    assert_eq!(contexts(&untyped, POTENTIAL), [fresh(Slot::NotSet)]);
}

#[test]
fn the_callers_self_link_selects_a_branch_in_the_wrapper() {
    let wrapper = sets_leader_when([
        (0x2008, "ldr", "x8,[x20,#0x30]"),
        (0x200c, "cmp", "x8,x20"),
        (0x2010, "b.ne", "#0x201c"),
    ]);
    let self_linked = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
        .method(&wrapper)
        .analyze();
    let linked = Program::new()
        .function(&links_root_with_its_own_prev())
        .method(&wrapper)
        .analyze();

    assert_eq!(contexts(&self_linked, POTENTIAL), [fresh(LEADER)]);
    assert_eq!(
        contexts(&linked, POTENTIAL),
        [Context {
            this: COUNTRY,
            root: LEADER,
            from: vec![Slot::SelfLink],
            prev: vec![Slot::SelfLink],
        }]
    );
}

/// A caller at 0x1000 that builds a country scope A, runs `between`, then calls the `potential`
/// wrapper at 0x2000 and the `allow` wrapper at 0x2100 with A.
fn evaluates_both(between: &[Line]) -> Vec<Line> {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend_from_slice(between);
    let next = lines.last().unwrap().0 + 4;
    lines.extend(calls_with_a(next, "#0x2000"));
    lines.extend(calls_with_a(next + 0xc, "#0x2100"));
    lines.push((next + 0x18, "ret", ""));
    lines
}

#[test]
fn a_scope_stays_known_from_one_evaluation_to_the_next() {
    let result = Program::new()
        .function(&evaluates_both(&[]))
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert_eq!(contexts(&result, ALLOW), [fresh(COUNTRY)]);
}

#[test]
fn a_scope_passed_to_an_unknown_call_between_evaluations_is_unresolved() {
    let between = [(0x1018, "add", "x0,sp,#0x100"), (0x101c, "bl", "#0x9900")];
    let result = Program::new()
        .function(&evaluates_both(&between))
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [unreadable()]);
    assert_eq!(contexts(&result, ALLOW), [unreadable()]);
}

#[test]
fn a_scope_that_a_reader_receives_between_evaluations_stays_known() {
    let between = [(0x1018, "add", "x2,sp,#0x100"), (0x101c, "bl", "#0x9700")];
    let run = |program: Program| {
        program
            .function(&evaluates_both(&between))
            .method(&wrapper(0x2000, POTENTIAL_OF_X19))
            .method(&wrapper(0x2100, ALLOW_OF_X19))
            .analyze()
    };

    assert_eq!(
        contexts(&run(Program::new().reader(0x9700)), ALLOW),
        [fresh(COUNTRY)]
    );
    assert_eq!(contexts(&run(Program::new()), ALLOW), [unreadable()]);
}

#[test]
fn a_change_after_the_evaluation_reaches_the_next_one() {
    let mut potential = wrapper(0x2000, POTENTIAL_OF_X19);
    potential.truncate(5);
    potential.extend([
        (0x2014, "mov", "x0,x20"),
        (0x2018, "bl", "#0x8200"),
        (0x201c, "ret", ""),
    ]);
    let result = Program::new()
        .function(&evaluates_both(&[]))
        .method(&potential)
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert_eq!(contexts(&result, ALLOW), [fresh(LEADER)]);
}

#[test]
fn a_change_on_an_early_return_reaches_the_next_evaluation() {
    let potential = [
        (0x2000, "mov", "x19,x0"),
        (0x2004, "mov", "x20,x1"),
        (0x2008, "cbz", "x2,#0x2020"),
        (0x200c, "add", "x0,x19,#0x40"),
        (0x2010, "mov", "x1,x20"),
        (0x2014, "bl", "#0x9400"),
        (0x2018, "ret", ""),
        (0x201c, "nop", ""),
        (0x2020, "mov", "x0,x20"),
        (0x2024, "bl", "#0x8200"),
        (0x2028, "ret", ""),
    ];
    let result = Program::new()
        .function(&evaluates_both(&[]))
        .method(&potential)
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, ALLOW), [fresh(COUNTRY), fresh(LEADER)]);
}

/// A `potential` wrapper that passes its scope to an unknown call, such as a tooltip builder,
/// after the evaluation and only when its third argument is not null.
fn passes_scope_on_when_x2_is_set() -> Vec<Line> {
    vec![
        (0x2000, "mov", "x19,x0"),
        (0x2004, "mov", "x20,x1"),
        (0x2008, "mov", "x22,x2"),
        (0x200c, "add", "x0,x19,#0x40"),
        (0x2010, "mov", "x1,x20"),
        (0x2014, "bl", "#0x9400"),
        (0x2018, "cbz", "x22,#0x2024"),
        (0x201c, "mov", "x0,x20"),
        (0x2020, "bl", "#0x9900"),
        (0x2024, "ret", ""),
    ]
}

#[test]
fn a_null_argument_from_the_caller_keeps_the_scope_from_a_tooltip_call() {
    let result = Program::new()
        .function(&evaluates_both(&[(0x1018, "mov", "x2,#0")]))
        .method(&passes_scope_on_when_x2_is_set())
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, ALLOW), [fresh(COUNTRY)]);
}

#[test]
fn a_set_argument_from_the_caller_passes_the_scope_to_a_tooltip_call() {
    let result = Program::new()
        .function(&evaluates_both(&[(0x1018, "add", "x2,sp,#0x20")]))
        .method(&passes_scope_on_when_x2_is_set())
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert_eq!(contexts(&result, ALLOW), [unreadable()]);
}

#[test]
fn a_flag_that_the_wrapper_writes_selects_the_callers_next_scope() {
    let potential = [
        (0x2000, "mov", "x19,x0"),
        (0x2004, "mov", "x20,x1"),
        (0x2008, "mov", "x9,#1"),
        (0x200c, "str", "x9,[x2]"),
        (0x2010, "add", "x0,x19,#0x40"),
        (0x2014, "mov", "x1,x20"),
        (0x2018, "bl", "#0x9400"),
        (0x201c, "ret", ""),
    ];
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "str", "xzr,[sp,#0x20]"),
        (0x101c, "mov", "x0,x21"),
        (0x1020, "add", "x1,sp,#0x100"),
        (0x1024, "add", "x2,sp,#0x20"),
        (0x1028, "bl", "#0x2000"),
        (0x102c, "ldr", "x8,[sp,#0x20]"),
        (0x1030, "cbz", "x8,#0x103c"),
        (0x1034, "add", "x0,sp,#0x100"),
        (0x1038, "bl", "#0x8200"),
    ]);
    lines.extend(calls_with_a(0x103c, "#0x2100"));
    lines.push((0x1048, "ret", ""));
    let result = Program::new()
        .arguments(EVALUATE, 2)
        .function(&lines)
        .method(&potential)
        .method(&wrapper(0x2100, ALLOW_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, ALLOW), [fresh(LEADER)]);
}

#[test]
fn an_evaluation_or_a_tooltip_call_forgets_the_local_that_it_receives() {
    let run = |call, passed| {
        let mut lines = builds_a(0x1000, COUNTRY_TYPE);
        lines.extend([
            (0x1018, "str", "xzr,[sp,#0x20]"),
            (0x101c, "add", passed),
            (0x1020, "bl", call),
            (0x1024, "ldr", "x8,[sp,#0x20]"),
            (0x1028, "cbz", "x8,#0x1034"),
            (0x102c, "add", "x0,sp,#0x100"),
            (0x1030, "bl", "#0x8200"),
        ]);
        lines.extend(calls_with_a(0x1034, "#0x2100"));
        lines.push((0x1040, "ret", ""));
        let result = Program::new()
            .function(&lines)
            .method(&wrapper(0x2100, ALLOW_OF_X19))
            .analyze();
        contexts(&result, ALLOW)
    };

    for call in ["#0x9400", "#0x9410"] {
        assert_eq!(run(call, "x1,sp,#0x20"), [fresh(COUNTRY), fresh(LEADER)]);
        assert_eq!(run(call, "x1,sp,#0x28"), [fresh(COUNTRY)]);
    }
}

/// A `potential` wrapper that stores its scope pointer where `x2` points when `keeps` is set.
fn stores_scope_pointer(keeps: bool) -> Vec<Line> {
    let store = if keeps {
        (0x2008, "str", "x20,[x2]")
    } else {
        (0x2008, "nop", "")
    };
    wrapper_doing(&[store])
}

#[test]
fn a_scope_pointer_that_the_wrapper_keeps_escapes_to_a_call_that_can_reach_it() {
    let mut caller = builds_a(0x1000, COUNTRY_TYPE);
    caller.extend([
        (0x1018, "add", "x2,sp,#0x20"),
        (0x101c, "mov", "x0,x21"),
        (0x1020, "add", "x1,sp,#0x100"),
        (0x1024, "bl", "#0x2000"),
        (0x1028, "add", "x0,sp,#0x20"), // where the wrapper kept the pointer
        (0x102c, "bl", "#0x9900"),
    ]);
    caller.extend(calls_with_a(0x1030, "#0x2100"));
    caller.push((0x103c, "ret", ""));
    let run = |keeps| {
        Program::new()
            .function(&caller)
            .method(&stores_scope_pointer(keeps))
            .method(&wrapper(0x2100, ALLOW_OF_X19))
            .analyze()
    };

    assert_eq!(contexts(&run(true), ALLOW), [unreadable()]);
    assert_eq!(contexts(&run(false), ALLOW), [fresh(COUNTRY)]);
}

#[test]
fn an_evaluation_before_the_entry_call_records_nothing_for_its_block() {
    let method = [
        (0x2000, "sub", "sp,sp,#0x100"),
        (0x2004, "mov", "x19,x0"),
        (0x2008, "mov", "x20,x1"),
        (0x200c, "add", "x0,x19,#0x40"),
        (0x2010, "mov", "x1,x20"),
        (0x2014, "bl", "#0x9400"), // potential, with the scope parameter
        (0x2018, "add", "x0,sp,#0x80"),
        (0x201c, "bl", "#0x8000"),
        (0x2020, "add", "x0,sp,#0x80"),
        (0x2024, "bl", "#0x8100"),
        (0x2028, "add", "x0,x19,#0x48"),
        (0x202c, "add", "x1,sp,#0x80"),
        (0x2030, "bl", "#0x9400"), // allow, with the method's own scope
        (0x2034, "add", "sp,sp,#0x100"),
        (0x2038, "ret", ""),
    ];
    let result = Program::new()
        .function(&caller(0x1000, LEADER_TYPE, "#0x2000"))
        .method(&method)
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(LEADER)]);
    assert!(unresolved(&result, POTENTIAL).is_empty());
    assert_eq!(contexts(&result, ALLOW), [fresh(COUNTRY)]);
}

/// A function at `base` that passes its receiver and scope on to `callee`.
fn forwards(base: u64, callee: &'static str) -> [Line; 2] {
    [(base, "bl", callee), (base + 4, "ret", "")]
}

#[test]
fn a_scope_forwarded_through_two_callers_is_read_where_it_is_built() {
    let result = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x3000"))
        .function(&forwards(0x3000, "#0x2000"))
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
}

#[test]
fn a_scope_forwarded_through_three_callers_is_past_the_caller_depth() {
    let result = Program::new()
        .function(&caller(0x1000, COUNTRY_TYPE, "#0x3100"))
        .function(&forwards(0x3100, "#0x3000"))
        .function(&forwards(0x3000, "#0x2000"))
        .method(&wrapper(0x2000, POTENTIAL_OF_X19))
        .analyze();

    assert!(contexts(&result, POTENTIAL).is_empty());
    assert_eq!(unresolved(&result, POTENTIAL), ["caller-depth"]);
}

#[test]
fn callers_with_different_scopes_give_two_contexts_and_equal_ones_merge() {
    let run = |second: (&'static str, &'static str)| {
        Program::new()
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .function(&caller(0x1100, second, "#0x2000"))
            .method(&wrapper(0x2000, POTENTIAL_OF_X19))
            .analyze()
    };

    assert_eq!(
        contexts(&run(LEADER_TYPE), POTENTIAL),
        [fresh(COUNTRY), fresh(LEADER)]
    );
    assert_eq!(contexts(&run(COUNTRY_TYPE), POTENTIAL), [fresh(COUNTRY)]);

    let runs: Vec<_> = run(LEADER_TYPE)
        .runs
        .into_iter()
        .map(|run| {
            let reached: Vec<_> = run.reached.into_iter().collect();
            (run.function, run.site, run.wrapper, reached)
        })
        .collect();
    assert_eq!(
        runs,
        [
            (
                0x1000,
                0x1020,
                Some(0x2000),
                vec![(0x2010, block(POTENTIAL), fresh(COUNTRY))]
            ),
            (
                0x1100,
                0x1120,
                Some(0x2000),
                vec![(0x2010, block(POTENTIAL), fresh(LEADER))]
            ),
        ]
    );
}

#[test]
fn a_call_does_not_receive_a_scope_left_in_a_register_that_its_signature_does_not_use() {
    let before = [(0x2008, "bl", "#0x9500")]; // x1 still holds the scope
    let run = |program: Program| {
        program
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .method(&wrapper_doing(&before))
            .analyze()
    };

    assert_eq!(
        contexts(&run(Program::new().arguments(0x9500, 0)), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(
        contexts(&run(Program::new().arguments(0x9500, 1)), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(
        contexts(&run(Program::new().arguments(0x9500, 2)), POTENTIAL),
        [unreadable()]
    );
    assert_eq!(contexts(&run(Program::new()), POTENTIAL), [unreadable()]);
}

#[test]
fn a_tooltip_builder_that_receives_the_scope_keeps_it_known() {
    let mut potential = wrapper(0x2000, POTENTIAL_OF_X19);
    potential.truncate(5);
    potential.extend([
        (0x2014, "mov", "x1,x20"),
        (0x2018, "bl", "#0x9410"),
        (0x201c, "ret", ""),
    ]);
    let unknown: Vec<Line> = potential
        .iter()
        .map(|&line| match line {
            (0x2018, "bl", _) => (0x2018, "bl", "#0x9900"),
            line => line,
        })
        .collect();
    let run = |potential: &[Line]| {
        Program::new()
            .function(&evaluates_both(&[]))
            .method(potential)
            .method(&wrapper(0x2100, ALLOW_OF_X19))
            .analyze()
    };

    assert_eq!(contexts(&run(&potential), ALLOW), [fresh(COUNTRY)]);
    assert_eq!(contexts(&run(&unknown), ALLOW), [unreadable()]);
}

/// A function at 0x3000 that receives a scope in `x1` and runs `body` on it in `x20`.
fn receives_scope(body: &[Line]) -> Vec<Line> {
    let mut lines = vec![(0x3000, "mov", "x20,x1")];
    lines.extend_from_slice(body);
    let next = lines.last().unwrap().0 + 4;
    lines.push((next, "ret", ""));
    lines
}

/// A caller at 0x1000 that builds a country scope A, passes it to the function at 0x3000 and
/// then to the `potential` wrapper at 0x2000.
fn passes_a_on_first() -> Vec<Line> {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([(0x1018, "add", "x1,sp,#0x100"), (0x101c, "bl", "#0x3000")]);
    lines.extend(calls_with_a(0x1020, "#0x2000"));
    lines.push((0x102c, "ret", ""));
    lines
}

#[test]
fn a_function_that_receives_the_scope_runs_on_the_path() {
    let evaluates = receives_scope(&[(0x3004, "mov", "x1,x20"), (0x3008, "bl", "#0x9400")]);
    let sets_leader = receives_scope(&[(0x3004, "mov", "x0,x20"), (0x3008, "bl", "#0x8200")]);
    let run = |program: Program| {
        program
            .function(&passes_a_on_first())
            .method(&wrapper(0x2000, POTENTIAL_OF_X19))
            .analyze()
    };

    assert_eq!(
        contexts(&run(Program::new().receiver(&evaluates)), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(
        contexts(&run(Program::new().receiver(&sets_leader)), POTENTIAL),
        [fresh(LEADER)]
    );
    assert_eq!(
        contexts(&run(Program::new().function(&evaluates)), POTENTIAL),
        [unreadable()]
    );
}

#[test]
fn a_call_that_never_returns_ends_its_path() {
    let ends = |call: &'static str| -> Vec<Line> {
        vec![
            (0x2000, "mov", "x19,x0"),
            (0x2004, "mov", "x20,x1"),
            (0x2008, "cbz", "x2,#0x201c"),
            (0x200c, "add", "x0,x19,#0x40"),
            (0x2010, "mov", "x1,x20"),
            (0x2014, "bl", "#0x9400"),
            (0x2018, "ret", ""),
            (0x201c, "bl", call), // the last instruction of the function
        ]
    };
    let run = |potential: Vec<Line>| {
        Program::new()
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .method(&potential)
            .analyze()
    };

    let never_returns = run(ends("#0x9420"));
    assert_eq!(contexts(&never_returns, POTENTIAL), [fresh(COUNTRY)]);
    assert!(unresolved(&never_returns, POTENTIAL).is_empty());
    assert_eq!(
        unresolved(&run(ends("#0x9900")), POTENTIAL),
        ["outside-code"]
    );
}

#[test]
fn a_call_through_a_pointer_reads_the_registers_that_the_binding_states() {
    let before = [(0x2008, "mov", "x8,x20"), (0x200c, "blr", "x16")]; // x1 and x8 hold the scope
    let run = |program: Program| {
        program
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .method(&wrapper_doing(&before))
            .analyze()
    };

    assert_eq!(
        contexts(&run(Program::new().call_arguments(0x200c, 1)), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(contexts(&run(Program::new()), POTENTIAL), [unreadable()]);
}

/// A `potential` wrapper with seven branches on unknown values before the evaluation, which
/// make more paths than a run follows. When `escapes` is set, the side of a last branch that the
/// search takes first passes the scope to an unknown call.
fn branches_before_evaluating(escapes: bool) -> Vec<Line> {
    let escape = if escapes {
        [(0x2028, "mov", "x0,x20"), (0x202c, "bl", "#0x9900")]
    } else {
        [(0x2028, "nop", ""), (0x202c, "nop", "")]
    };
    let mut lines = vec![
        (0x2000, "mov", "x19,x0"),
        (0x2004, "mov", "x20,x1"),
        (0x2008, "cbz", "x3,#0x200c"),
        (0x200c, "cbz", "x4,#0x2010"),
        (0x2010, "cbz", "x5,#0x2014"),
        (0x2014, "cbz", "x6,#0x2018"),
        (0x2018, "cbz", "x7,#0x201c"),
        (0x201c, "cbz", "x9,#0x2020"),
        (0x2020, "cbz", "x2,#0x2028"),
        (0x2024, "b", "#0x2030"),
    ];
    lines.extend(escape);
    lines.extend([
        (0x2030, "add", "x0,x19,#0x40"),
        (0x2034, "mov", "x1,x20"),
        (0x2038, "bl", "#0x9400"),
        (0x203c, "ret", ""),
    ]);
    lines
}

#[test]
fn a_search_that_ends_at_a_bound_with_no_contradiction_has_no_gap() {
    let run = |escapes| {
        Program::new()
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .method(&branches_before_evaluating(escapes))
            .analyze()
    };

    let bounds = |result: &BlockEntries| -> BTreeSet<(&'static str, bool, bool)> {
        result
            .runs
            .iter()
            .flat_map(|run| {
                run.bounded
                    .iter()
                    .map(|bound| (bound.reason, bound.stop.is_some(), run.contradicted))
            })
            .collect()
    };

    let clean = run(false);
    assert_eq!(contexts(&clean, POTENTIAL), [fresh(COUNTRY)]);
    assert!(unresolved(&clean, POTENTIAL).is_empty());
    assert_eq!(
        bounds(&clean),
        BTreeSet::from([("path-limit", true, false)])
    );

    let contradicted = run(true);
    assert_eq!(
        contexts(&contradicted, POTENTIAL),
        [fresh(COUNTRY), unreadable()]
    );
    assert_eq!(unresolved(&contradicted, POTENTIAL), ["path-limit"]);
    assert_eq!(
        bounds(&contradicted),
        BTreeSet::from([("path-limit", true, true)])
    );
}

#[test]
fn a_scope_pointer_kept_in_the_callers_stack_escapes_only_to_a_call_that_can_reach_it() {
    let run = |unknown_call_receives: (&'static str, &'static str)| {
        let mut lines = builds_a(0x1000, COUNTRY_TYPE);
        lines.extend([
            (0x1018, "add", "x9,sp,#0x100"),
            (0x101c, "str", "x9,[sp,#0x20]"), // a copy of A's address in the caller's frame
            (0x1020, unknown_call_receives.0, unknown_call_receives.1),
            (0x1024, "mov", "x8,#0"),
            (0x1028, "bl", "#0x9900"),
        ]);
        lines.extend(calls_with_a(0x102c, "#0x2000"));
        lines.push((0x1038, "ret", ""));
        Program::new()
            .function(&lines)
            .method(&wrapper(0x2000, POTENTIAL_OF_X19))
            .analyze()
    };

    assert_eq!(
        contexts(&run(("mov", "x0,#0")), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(
        contexts(&run(("add", "x0,sp,#0x10")), POTENTIAL),
        [unreadable()]
    );
}

#[test]
fn a_call_that_receives_an_address_in_an_inner_frame_does_not_reach_an_outer_frame() {
    let run = |passed: (&'static str, &'static str)| {
        let mut caller = builds_a(0x1000, COUNTRY_TYPE);
        caller.extend([
            (0x1018, "add", "x9,sp,#0x100"),
            (0x101c, "str", "x9,[sp,#0x20]"), // a copy of A's address in the caller's frame
            (0x1020, "add", "x2,sp,#0x10"),
        ]);
        caller.extend(calls_with_a(0x1024, "#0x2000"));
        caller.push((0x1030, "ret", ""));
        let before = [
            (0x2008, "sub", "sp,sp,#0x40"),
            (0x200c, passed.0, passed.1),
            (0x2010, "mov", "x1,#0"),
            (0x2014, "mov", "x2,#0"),
            (0x2018, "mov", "x8,#0"),
            (0x201c, "bl", "#0x9900"),
            (0x2020, "add", "sp,sp,#0x40"),
        ];
        Program::new()
            .function(&caller)
            .method(&wrapper_doing(&before))
            .analyze()
    };

    assert_eq!(
        contexts(&run(("add", "x0,sp,#0x10")), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(contexts(&run(("mov", "x0,x2")), POTENTIAL), [unreadable()]);
}

/// A caller that builds scope A, keeps its address in `x20` and calls the wrapper at 0x2000 with
/// it.
fn caller_keeping_a_in_x20() -> Vec<Line> {
    let mut lines = builds_a(0x1000, COUNTRY_TYPE);
    lines.extend([
        (0x1018, "add", "x20,sp,#0x100"),
        (0x101c, "mov", "x0,x21"),
        (0x1020, "mov", "x1,x20"),
        (0x1024, "bl", "#0x2000"),
        (0x1028, "ret", ""),
    ]);
    lines
}

/// A wrapper at 0x2000 that runs `entry`, keeps its receiver and scope, passes the local at
/// sp+0x8 to an unknown call that reads one argument, and evaluates `potential`.
fn wrapper_with_a_local(entry: &[(&'static str, &'static str)]) -> Vec<Line> {
    let mut lines: Vec<Line> = entry
        .iter()
        .map(|(operation, operands)| (0, *operation, *operands))
        .collect();
    lines.extend([
        (0, "mov", "x19,x0"),
        (0, "mov", "x20,x1"),
        (0, "add", "x0,sp,#0x8"), // a local below the saves
        (0, "mov", "x8,#0"),
        (0, "bl", "#0x9900"),
        (0, "add", "x0,x19,#0x40"),
        (0, "mov", "x1,x20"),
        (0, "bl", "#0x9400"),
        (0, "ldp", "x29,x30,[sp,#0x30]"),
        (0, "ldp", "x20,x19,[sp,#0x20]"),
        (0, "add", "sp,sp,#0x40"),
        (0, "ret", ""),
    ]);
    for (index, line) in lines.iter_mut().enumerate() {
        line.0 = 0x2000 + 4 * index as u64;
    }
    lines
}

const PROLOGUE: &[(&str, &str)] = &[
    ("sub", "sp,sp,#0x40"),
    ("stp", "x20,x19,[sp,#0x20]"), // the caller's x20: scope A
    ("stp", "x29,x30,[sp,#0x30]"),
    ("add", "x29,sp,#0x30"),
];

#[test]
fn a_prologue_save_of_the_callers_scope_does_not_reach_a_call_that_gets_a_lower_local() {
    let result = Program::new()
        .function(&caller_keeping_a_in_x20())
        .method(&wrapper_with_a_local(PROLOGUE))
        .arguments(0x9900, 1)
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
}

#[test]
fn a_save_after_the_prologue_reaches_a_call_that_gets_a_lower_local() {
    let late_save = [
        ("sub", "sp,sp,#0x40"),
        ("mov", "x9,x0"),
        ("stp", "x20,x19,[sp,#0x20]"), // after a body row: an ordinary store
        ("stp", "x29,x30,[sp,#0x30]"),
    ];
    let result = Program::new()
        .function(&caller_keeping_a_in_x20())
        .method(&wrapper_with_a_local(&late_save))
        .arguments(0x9900, 1)
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [unreadable()]);
}

#[test]
fn a_body_spill_of_the_scope_reaches_a_call_that_gets_a_lower_local() {
    let mut spills = PROLOGUE.to_vec();
    spills.extend([("mov", "x22,x1"), ("str", "x22,[sp,#0x18]")]); // the scope in the frame
    let result = Program::new()
        .function(&caller_keeping_a_in_x20())
        .method(&wrapper_with_a_local(&spills))
        .arguments(0x9900, 1)
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [unreadable()]);
}

/// The instance pointer at 0x9d50, named by the slot at 0x7248, holds an object whose vtable
/// point is 0x6010; that vtable's slot +0x40 holds the function at 0x3000.
const INSTANCE_POINTER: u64 = 0x9d50;
const INSTANCE_WORDS: &[(u64, u64)] = &[(0x7248, INSTANCE_POINTER), (0x6050, 0x3000)];

/// Before evaluating, the wrapper calls slot +0x40 of the object that the instance pointer holds,
/// while `x1` holds its scope.
const CALLS_THE_INSTANCE: [Line; 7] = [
    (0x2008, "adrp", "x9,#0x7000"),
    (0x200c, "ldr", "x9,[x9,#0x248]"),
    (0x2010, "ldr", "x0,[x9]"),
    (0x2014, "ldr", "x8,[x0]"),
    (0x2018, "ldr", "x8,[x8,#0x40]"),
    (0x201c, "mov", "x1,x20"),
    (0x2020, "blr", "x8"),
];

#[test]
fn a_virtual_call_on_a_proven_instance_reads_the_registers_of_its_target() {
    let run = |program: Program| {
        program
            .function(&caller(0x1000, COUNTRY_TYPE, "#0x2000"))
            .method(&wrapper_doing(&CALLS_THE_INSTANCE))
            .analyze()
    };
    let proven = || Program::new().instance(INSTANCE_POINTER, 0x6010, INSTANCE_WORDS);

    assert_eq!(
        contexts(&run(proven().arguments(0x3000, 1)), POTENTIAL),
        [fresh(COUNTRY)]
    );
    assert_eq!(
        contexts(&run(proven().arguments(0x3000, 2)), POTENTIAL),
        [unreadable()]
    );
    assert_eq!(
        contexts(&run(Program::new().arguments(0x3000, 1)), POTENTIAL),
        [unreadable()]
    );
}

#[test]
fn a_caller_that_builds_one_scope_and_forwards_another_adds_nothing_to_the_forwarded_block() {
    let wrapper = [
        (0x2000, "mov", "x19,x0"),
        (0x2004, "mov", "x20,x1"),
        (0x2008, "mov", "x21,x2"),
        (0x200c, "add", "x0,x19,#0x40"),
        (0x2010, "mov", "x1,x20"),
        (0x2014, "bl", "#0x9400"),
        (0x2018, "add", "x0,x19,#0x48"),
        (0x201c, "mov", "x1,x21"),
        (0x2020, "bl", "#0x9400"),
        (0x2024, "ret", ""),
    ];
    let builds_potential_forwards_allow = [
        (0x1000, "sub", "sp,sp,#0x200"),
        (0x1004, "mov", "x21,x0"),
        (0x1008, "mov", "x22,x2"),
        (0x100c, "add", "x0,sp,#0x100"),
        (0x1010, "bl", "#0x8000"),
        (0x1014, "add", "x0,sp,#0x100"),
        (0x1018, "bl", "#0x8100"),
        (0x101c, "mov", "x0,x21"),
        (0x1020, "add", "x1,sp,#0x100"),
        (0x1024, "mov", "x2,x22"),
        (0x1028, "bl", "#0x2000"),
        (0x102c, "ret", ""),
    ];
    let mut builds_allow = builds_a(0x3000, LEADER_TYPE);
    builds_allow.extend([
        (0x3018, "mov", "x0,x21"),
        (0x301c, "add", "x2,sp,#0x100"),
        (0x3020, "bl", "#0x1000"),
        (0x3024, "ret", ""),
    ]);
    let result = Program::new()
        .function(&builds_allow)
        .function(&builds_potential_forwards_allow)
        .method(&wrapper)
        .analyze();

    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert_eq!(contexts(&result, ALLOW), [fresh(LEADER)]);
    assert!(unresolved(&result, ALLOW).is_empty());
}

/// A function at 0x1000 that runs `setup`, builds country scope A at sp+0x100 in registers that
/// `setup` does not use, and runs `call` with A in `x1`.
fn with_country_scope(
    setup: &[(&'static str, &'static str)],
    call: &[(&'static str, &'static str)],
) -> Vec<Line> {
    let scope = [
        ("add", "x0,sp,#0x100"),
        ("bl", "#0x8000"),
        ("add", "x0,sp,#0x100"),
        COUNTRY_TYPE,
        ("add", "x1,sp,#0x100"),
    ];
    std::iter::once(("sub", "sp,sp,#0x200"))
        .chain(setup.iter().copied())
        .chain(scope)
        .chain(call.iter().copied())
        .chain([("ret", "")])
        .zip((0x1000..).step_by(4))
        .map(|((operation, operands), address)| (address, operation, operands))
        .collect()
}

/// The registers of a function that lead to the owner: register `register`, or the word at it
/// plus `member`.
fn leading(register: usize, member: Option<i64>) -> TypePointers {
    match member {
        None => TypePointers {
            registers: BTreeMap::from([(register, OWNER.into())]),
            ..TypePointers::default()
        },
        Some(member) => TypePointers {
            members: BTreeMap::from([((register, member), OWNER.into())]),
            ..TypePointers::default()
        },
    }
}

#[test]
fn an_evaluation_through_the_vtable_of_a_block_of_an_owner_parameter_reaches_it() {
    let lines = with_country_scope(
        &[("mov", "x21,x1")],
        &[
            ("add", "x0,x21,#0x2e0"),
            ("ldr", "x8,[x21,#0x2e0]"),
            ("ldr", "x8,[x8,#0x48]"),
            ("blr", "x8"),
        ],
    );
    let result = Program::new().pointing(leading(1, None), &lines).analyze();

    assert_eq!(block_contexts(&result, &effect(0x2e0)), [fresh(COUNTRY)]);
    assert!(result.unattributed.is_empty());
}

#[test]
fn an_evaluation_through_the_vtable_of_a_block_of_a_member_of_the_receiver_reaches_it() {
    let start = |vtable: &'static str| {
        with_country_scope(
            &[("mov", "x19,x0")],
            &[
                ("ldr", "x8,[x19,#0x18]"),
                ("add", "x0,x8,#0x190"),
                ("ldr", vtable),
                ("ldr", "x8,[x8,#0x48]"),
                ("blr", "x8"),
            ],
        )
    };
    let folded = Program::new()
        .pointing(leading(0, Some(0x18)), &start("x8,[x8,#0x190]"))
        .analyze();
    let through_x0 = Program::new()
        .pointing(leading(0, Some(0x18)), &start("x8,[x0]"))
        .analyze();

    assert_eq!(block_contexts(&folded, &effect(0x190)), [fresh(COUNTRY)]);
    assert_eq!(
        block_contexts(&through_x0, &effect(0x190)),
        [fresh(COUNTRY)]
    );
}

#[test]
fn a_member_of_a_parameter_kept_in_a_callee_saved_register_leads_to_the_block() {
    let lines = with_country_scope(
        &[("mov", "x20,x1"), ("ldr", "x23,[x20,#0x18]")],
        &[
            ("add", "x0,x23,#0x3a8"),
            ("ldr", "x8,[x23,#0x3a8]"),
            ("ldr", "x8,[x8,#0x48]"),
            ("blr", "x8"),
        ],
    );
    let result = Program::new()
        .pointing(leading(1, Some(0x18)), &lines)
        .analyze();

    assert_eq!(block_contexts(&result, &effect(0x3a8)), [fresh(COUNTRY)]);
}

#[test]
fn a_direct_evaluation_outside_the_owners_methods_reaches_the_block_of_a_parameter_or_member() {
    let parameter = with_country_scope(
        &[("mov", "x22,x1")],
        &[("add", "x0,x22,#0x40"), ("bl", "#0x9400")],
    );
    let member = with_country_scope(
        &[("ldr", "x22,[x1,#0x18]")],
        &[("add", "x0,x22,#0x40"), ("bl", "#0x9400")],
    );

    let by_parameter = Program::new()
        .pointing(leading(1, None), &parameter)
        .analyze();
    let by_member = Program::new()
        .pointing(leading(1, Some(0x18)), &member)
        .analyze();

    assert_eq!(contexts(&by_parameter, POTENTIAL), [fresh(COUNTRY)]);
    assert_eq!(contexts(&by_member, POTENTIAL), [fresh(COUNTRY)]);
    assert!(by_parameter.unattributed.is_empty());
}

#[test]
fn a_block_of_an_object_whose_owner_is_not_established_is_neither_named_nor_counted() {
    let looked_up = with_country_scope(
        &[("bl", "#0x9900"), ("mov", "x22,x0")],
        &[("add", "x0,x22,#0x40"), ("bl", "#0x9400")],
    );
    let other_register = with_country_scope(
        &[("mov", "x22,x2")],
        &[("add", "x0,x22,#0x40"), ("bl", "#0x9400")],
    );

    for lines in [looked_up, other_register] {
        let result = Program::new().pointing(leading(1, None), &lines).analyze();
        assert!(result.blocks.is_empty());
        assert!(result.unattributed.is_empty());
    }
}

#[test]
fn a_call_through_another_slot_or_another_objects_vtable_or_on_the_item_is_not_an_evaluation() {
    let calls = |object: &'static str, vtable: &'static str, slot: &'static str| {
        let lines = with_country_scope(
            &[("mov", "x21,x0")],
            &[
                ("add", object),
                ("ldr", vtable),
                ("ldr", slot),
                ("blr", "x8"),
            ],
        );
        Program::new().method(&lines).analyze()
    };

    let tooltip = calls("x0,x21,#0x40", "x8,[x21,#0x40]", "x8,[x8,#0x58]");
    let other_object = calls("x0,x21,#0x40", "x8,[x21,#0x48]", "x8,[x8,#0x10]");
    let the_item = calls("x0,x21,#0", "x8,[x21]", "x8,[x8,#0x10]");
    let evaluation = calls("x0,x21,#0x40", "x8,[x21,#0x40]", "x8,[x8,#0x10]");

    for result in [tooltip, other_object, the_item] {
        assert!(result.blocks.is_empty());
        assert!(result.unattributed.is_empty());
    }
    assert_eq!(contexts(&evaluation, POTENTIAL), [fresh(COUNTRY)]);
}

#[test]
fn a_virtual_evaluation_leaves_the_scope_unknown_for_the_next_evaluation() {
    let lines = with_country_scope(
        &[("mov", "x21,x0")],
        &[
            ("add", "x0,x21,#0x48"),
            ("ldr", "x8,[x21,#0x48]"),
            ("ldr", "x8,[x8,#0x48]"),
            ("blr", "x8"),
            ("add", "x0,x21,#0x40"),
            ("add", "x1,sp,#0x100"),
            ("bl", "#0x9400"),
        ],
    );
    let result = Program::new().method(&lines).analyze();

    assert_eq!(block_contexts(&result, &effect(ALLOW)), [fresh(COUNTRY)]);
    assert!(contexts(&result, ALLOW).is_empty());
    assert_eq!(contexts(&result, POTENTIAL), [unreadable()]);
}

#[test]
fn a_constructor_stores_its_parameters_in_its_object() {
    let lines = [
        (0x1000, "stp", "x8,x2,[x0,#0x10]"),
        (0x1004, "mov", "x19,x0"),
        (0x1008, "mov", "x20,x3"),
        (0x100c, "str", "x1,[x19,#0x28]"),
        (0x1010, "bl", "#0x9900"),
        (0x1014, "str", "x3,[x19,#0x30]"),
        (0x1018, "str", "x20,[x19,#0x38]"),
        (0x101c, "str", "w4,[x19,#0x40]"),
        (0x1020, "ret", ""),
    ];

    assert_eq!(
        receiver_stores(&rows(&lines)),
        BTreeMap::from([
            (0x18, BTreeSet::from([2])),
            (0x28, BTreeSet::from([1])),
            (0x38, BTreeSet::from([3])),
        ])
    );
}

#[test]
fn a_call_to_an_offset_getter_names_the_block_at_its_offset() {
    let lines = with_country_scope(
        &[("mov", "x21,x1")],
        &[
            ("mov", "x0,x21"),
            ("bl", "#0x7000"),
            ("ldr", "x8,[x0]"),
            ("ldr", "x8,[x8,#0x48]"),
            ("add", "x1,sp,#0x100"),
            ("blr", "x8"),
        ],
    );
    let run = |program: Program| {
        program
            .arguments(0x7000, 1)
            .pointing(leading(1, None), &lines)
            .analyze()
    };

    let getter = run(Program::new().getter(0x7000, 0xa0));
    let other_call = run(Program::new());

    assert_eq!(block_contexts(&getter, &effect(0xa0)), [fresh(COUNTRY)]);
    assert!(other_call.blocks.is_empty());
}

/// A helper at 0x2000 that keeps the block in its parameter `x1`, builds its own scope, typed as
/// a country when `x2` is not zero and as a leader otherwise, and runs the block's effect. Its
/// receiver leads to the owner through the member at `+0x18`, as `CMission`'s does.
const HELPER: Rows<'static> = &[
    (0x2000, "sub", "sp,sp,#0x200"),
    (0x2004, "mov", "x19,x1"),
    (0x2008, "mov", "x20,x2"),
    (0x200c, "add", "x0,sp,#0x100"),
    (0x2010, "bl", "#0x8000"),
    (0x2014, "add", "x0,sp,#0x100"),
    (0x2018, "cbz", "x20,#0x2024"),
    (0x201c, "bl", "#0x8100"),
    (0x2020, "b", "#0x2028"),
    (0x2024, "bl", "#0x8200"),
    (0x2028, "ldr", "x8,[x19]"),
    (0x202c, "ldr", "x8,[x8,#0x48]"),
    (0x2030, "mov", "x0,x19"),
    (0x2034, "add", "x1,sp,#0x100"),
    (0x2038, "blr", "x8"),
    (0x203c, "ret", ""),
];

/// A method of the owner at `base` that passes its block at `block` (`x1,x0,#offset`) and the
/// type flag `flag` (`w2,#flag`) to the helper.
fn passes_to_helper(base: u64, block: &'static str, flag: &'static str) -> Vec<Line> {
    vec![
        (base, "add", block),
        (base + 4, "mov", flag),
        (base + 8, "bl", "#0x2000"),
        (base + 12, "ret", ""),
    ]
}

#[test]
fn a_helper_gives_each_caller_s_block_only_the_context_of_that_caller_s_run() {
    let result = Program::new()
        .pointing(leading(0, Some(0x18)), HELPER)
        .method(&passes_to_helper(0x1000, "x1,x0,#0x40", "w2,#1"))
        .method(&passes_to_helper(0x1100, "x1,x0,#0x48", "w2,#0"))
        .analyze();

    assert_eq!(block_contexts(&result, &effect(0x40)), [fresh(COUNTRY)]);
    assert_eq!(block_contexts(&result, &effect(0x48)), [fresh(LEADER)]);
    assert!(result.helpers[&0x2000].is_empty());
}

#[test]
fn a_helper_with_no_caller_or_with_callers_that_name_no_block_gives_no_context() {
    let alone = Program::new()
        .pointing(leading(0, Some(0x18)), HELPER)
        .analyze();
    let passes_on = Program::new()
        .pointing(leading(0, Some(0x18)), HELPER)
        .function(&[(0x1000, "mov", "w2,#1"), (0x1004, "bl", "#0x2000")])
        .analyze();
    let unnamed = Program::new()
        .pointing(leading(0, Some(0x18)), HELPER)
        .method(&passes_to_helper(0x1000, "x1,x3,#0x40", "w2,#1"))
        .analyze();

    for result in [&alone, &passes_on, &unnamed] {
        assert!(result.blocks.is_empty());
    }
    assert_eq!(alone.helpers[&0x2000], BTreeMap::from([("no-caller", 1)]));
    assert_eq!(
        passes_on.helpers[&0x2000],
        BTreeMap::from([("block-caller-depth", 1)])
    );
    assert_eq!(
        unnamed.helpers[&0x2000],
        BTreeMap::from([("unattributed", 1)])
    );
}

#[test]
fn a_helper_whose_scope_is_a_parameter_too_charges_the_callers_block() {
    let helper = [
        (0x2000, "ldr", "x8,[x1]"),
        (0x2004, "ldr", "x8,[x8,#0x48]"),
        (0x2008, "mov", "x0,x1"),
        (0x200c, "mov", "x1,x2"),
        (0x2010, "blr", "x8"),
        (0x2014, "ret", ""),
    ];
    let caller = with_country_scope(
        &[("mov", "x21,x0")],
        &[("mov", "x2,x1"), ("add", "x1,x21,#0x40"), ("bl", "#0x2000")],
    );
    let result = Program::new()
        .pointing(leading(0, Some(0x18)), &helper)
        .method(&caller)
        .analyze();

    assert!(block_contexts(&result, &effect(0x40)).is_empty());
    assert_eq!(
        result.blocks[&effect(0x40)].unresolved,
        BTreeSet::from(["block-and-scope-from-caller"])
    );
}

#[test]
fn a_tooltip_call_names_its_block_without_an_evaluation() {
    let lines = with_country_scope(
        &[("mov", "x21,x0")],
        &[
            ("add", "x0,x21,#0x40"),
            ("ldr", "x8,[x21,#0x40]"),
            ("ldr", "x8,[x8,#0x58]"),
            ("blr", "x8"),
            ("add", "x0,x21,#0x48"),
            ("add", "x1,sp,#0x100"),
            ("bl", "#0x9410"),
            ("ldr", "x0,[x21,#0x50]"),
            ("add", "x1,sp,#0x100"),
            ("bl", "#0x9400"),
        ],
    );
    let result = Program::new().method(&lines).analyze();

    assert_eq!(
        result.tooltips,
        BTreeSet::from([block(POTENTIAL), block(ALLOW)])
    );
    assert!(result.blocks.is_empty());
    assert_eq!(result.unattributed[OWNER], 1);
}

#[test]
fn a_pre_index_load_of_the_vtable_names_the_block_at_its_offset() {
    let lines = with_country_scope(
        &[("mov", "x19,x2")],
        &[
            ("ldr", "x0,[x19,#0x18]"),
            ("ldr", "x8,[x0,#0x40]!"),
            ("ldr", "x8,[x8,#0x48]"),
            ("add", "x1,sp,#0x100"),
            ("blr", "x8"),
        ],
    );
    let result = Program::new()
        .pointing(leading(2, Some(0x18)), &lines)
        .analyze();

    assert_eq!(block_contexts(&result, &effect(0x40)), [fresh(COUNTRY)]);
}

fn authored_selection(bytes: &[u8]) -> Program {
    use crate::engine::analysis::decode::decode_arm64;
    let mut program = Program::new();
    program
        .functions
        .insert(0x1000, decode_arm64(bytes, 0x1000).unwrap());
    program.type_pointers.insert(
        0x1000,
        TypePointers {
            method_of: Some(OWNER.into()),
            registers: BTreeMap::from([(0, OWNER.into())]),
            members: BTreeMap::new(),
        },
    );
    program
}

#[test]
fn a_conditional_selection_keeps_each_blocks_country_or_planet_context() {
    use crate::engine::analysis::assembler::arm64;
    let bytes = arm64!(at 0x1000;
        mov x21, x0;
        sub sp, sp, #0x200;
        add x0, sp, #0x100;
        bl extern FRESH as usize;
        add x0, sp, #0x100;
        bl extern SET_COUNTRY as usize;
        add x0, sp, #0x180;
        bl extern FRESH as usize;
        mov w9, #2; // planet type bit 1
        str x9, [sp, #0x188];
        cbz x2, >planet;
        mov w20, #0;
        b >select;
        planet:;
        mov w20, #1;
        select:;
        cmp w20, #0;
        add x8, x21, #0x40;
        add x9, x21, #0x48;
        csel x0, x8, x9, eq;
        add x8, sp, #0x100;
        add x9, sp, #0x180;
        csel x1, x8, x9, eq;
        ldr x8, [x0];
        ldr x8, [x8, #0x10];
        blr x8;
        ret
    );
    let result = authored_selection(&bytes).analyze();
    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert_eq!(contexts(&result, ALLOW), [fresh(Slot::Scope(1))]);
    assert!(
        result
            .blocks
            .values()
            .all(|finding| finding.unresolved.is_empty())
    );
}

#[test]
fn an_unknown_alternative_keeps_the_known_block_and_an_identity_gap() {
    use crate::engine::analysis::assembler::arm64;
    let bytes = arm64!(at 0x1000;
        mov x21, x0;
        sub sp, sp, #0x200;
        add x0, sp, #0x100;
        bl extern FRESH as usize;
        add x0, sp, #0x100;
        bl extern SET_COUNTRY as usize;
        add x8, x21, #0x40;
        ldr x9, [x21, #0x80]; // unknown swap
        cmp x2, #0;
        csel x0, x8, x9, eq;
        add x1, sp, #0x100;
        ldr x8, [x0];
        ldr x8, [x8, #0x10];
        blr x8;
        ret
    );
    let result = authored_selection(&bytes).analyze();
    assert_eq!(contexts(&result, POTENTIAL), [fresh(COUNTRY)]);
    assert!(
        result.blocks[&block(POTENTIAL)]
            .unresolved
            .contains("selected-block-identity")
    );
}

#[test]
fn crossed_virtual_targets_do_not_attribute_selected_blocks() {
    use crate::engine::analysis::assembler::arm64;
    let bytes = arm64!(at 0x1000;
        mov x21, x0;
        sub sp, sp, #0x200;
        add x0, sp, #0x100;
        bl extern FRESH as usize;
        add x0, sp, #0x100;
        bl extern SET_COUNTRY as usize;
        add x8, x21, #0x40;
        add x9, x21, #0x48;
        cmp x2, #0;
        csel x0, x8, x9, eq;
        csel x10, x9, x8, eq;
        ldr x8, [x10];
        ldr x8, [x8, #0x10];
        add x1, sp, #0x100;
        blr x8;
        ret
    );
    let result = authored_selection(&bytes).analyze();
    assert!(result.blocks.is_empty());
}

#[test]
fn a_scope_stored_in_the_seeded_receiver_escapes_at_an_opaque_call() {
    use crate::engine::analysis::assembler::arm64;
    let bytes = arm64!(at 0x1000;
        mov x21, x0;
        sub sp, sp, #0x200;
        add x0, sp, #0x100;
        bl extern FRESH as usize;
        add x0, sp, #0x100;
        bl extern SET_COUNTRY as usize;
        add x8, sp, #0x100;
        str x8, [x21, #0x100];
        mov x0, x21;
        bl extern 0x9900;
        add x8, x21, #0x40;
        add x9, x21, #0x48;
        cmp x2, #0;
        csel x0, x8, x9, eq;
        add x1, sp, #0x100;
        bl extern EVALUATE as usize;
        ret
    );
    let result = authored_selection(&bytes).analyze();
    assert_eq!(contexts(&result, POTENTIAL), [unreadable()]);
    assert_eq!(contexts(&result, ALLOW), [unreadable()]);
}

#[test]
fn a_selected_blocks_parameter_scope_keeps_a_narrow_gap() {
    use crate::engine::analysis::assembler::arm64;
    let bytes = arm64!(at 0x1000;
        add x8, x0, #0x40;
        add x9, x0, #0x48;
        cmp x2, #0;
        csel x0, x8, x9, eq;
        bl extern EVALUATE as usize;
        ret
    );
    let result = authored_selection(&bytes).analyze();
    assert_eq!(result.blocks.len(), 2);
    for finding in result.blocks.values() {
        assert!(finding.contexts.is_empty());
        assert_eq!(
            finding.unresolved,
            BTreeSet::from(["selected-block-scope-from-caller"])
        );
    }
}

#[test]
fn a_selected_virtual_blocks_slot_supplies_only_its_own_family() {
    use crate::engine::analysis::assembler::arm64;
    let bytes = arm64!(at 0x1000;
        mov x21, x0;
        sub sp, sp, #0x200;
        add x0, sp, #0x100;
        bl extern FRESH as usize;
        add x0, sp, #0x100;
        bl extern SET_COUNTRY as usize;
        add x8, x21, #0x40;
        add x9, x21, #0x48;
        cmp x2, #0;
        csel x0, x8, x9, eq;
        add x1, sp, #0x100;
        ldr x8, [x0];
        ldr x8, [x8, #0x48]; // effect Execute, not a trigger evaluation
        blr x8;
        ret
    );
    let result = authored_selection(&bytes).analyze();
    assert_eq!(
        block_contexts(&result, &effect(POTENTIAL)),
        [fresh(COUNTRY)]
    );
    assert!(contexts(&result, POTENTIAL).is_empty());
    assert!(contexts(&result, ALLOW).is_empty());
}

#[test]
fn a_truncated_virtual_target_does_not_prove_a_selected_blocks_evaluation() {
    use crate::engine::analysis::assembler::arm64;
    let bytes = arm64!(at 0x1000;
        mov x21, x0;
        sub sp, sp, #0x200;
        add x0, sp, #0x100;
        bl extern FRESH as usize;
        add x0, sp, #0x100;
        bl extern SET_COUNTRY as usize;
        add x8, x21, #0x40;
        add x9, x21, #0x48;
        cmp x2, #0;
        csel x0, x8, x9, eq;
        add x1, sp, #0x100;
        ldr x8, [x0];
        mov w8, w8; // loses the upper half of the vtable address
        ldr x8, [x8, #0x48];
        blr x8;
        ret
    );
    assert!(authored_selection(&bytes).analyze().blocks.is_empty());
}
